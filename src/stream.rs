use crate::types::*;
use serde_json::{json, Value};
use worker::*;

/// Handle streaming via the Cloudflare REST API for true token-by-token SSE.
///
/// The `worker` crate's typed `Ai::run()` deserializes the response, losing the
/// SSE stream. We call the REST API directly to get raw streaming.
///
/// Requires `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` env vars.
pub async fn handle_streaming(
    req: &AnthropicRequest,
    workers_model: &str,
    workers_input: &Value,
    env: &Env,
) -> Result<Response> {
    let account_id = env.var("CLOUDFLARE_ACCOUNT_ID").ok().map(|v| v.to_string());
    let api_token = env.var("CLOUDFLARE_API_TOKEN").ok().map(|v| v.to_string());

    if let (Some(aid), Some(token)) = (account_id, api_token) {
        // REST API path — true streaming
        rest_streaming(&aid, &token, workers_model, workers_input, &req.model).await
    } else {
        // Fallback: use binding (no true streaming, all events in one chunk)
        let ai: Ai = env.ai("AI")?;
        let mut input = workers_input.clone();
        input["stream"] = json!(true);
        let result: Value = match ai.run(workers_model, input, AiOptions::default()).await {
            Ok(r) => r,
            Err(e) => {
                return crate::error_response(
                    "api_error",
                    &format!("Workers AI inference failed: {e}"),
                    Some(502),
                );
            }
        };
        binding_streaming_response(&result, &req.model).await
    }
}

/// True streaming via Cloudflare REST API.
async fn rest_streaming(
    account_id: &str,
    api_token: &str,
    model: &str,
    input: &Value,
    anthropic_model: &str,
) -> Result<Response> {
    let url = format!(
        "https://api.cloudflare.com/client/v4/accounts/{account_id}/ai/run/{model}"
    );

    let mut headers = Headers::new();
    headers.set("Authorization", &format!("Bearer {api_token}"))?;
    headers.set("Content-Type", "application/json")?;

    let mut fetch_req = Request::new_with_init(
        &url,
        RequestInit::new()
            .with_method(Method::Post)
            .with_headers(headers)
            .with_body(Some(Body::from_json(input)?)),
    )?;

    let resp = fetch(fetch_req).await.map_err(|e| {
        Error::RustError(format!("Failed to call Workers AI REST API: {e}"))
    })?;

    if !resp.status().is_success() {
        let status = resp.status();
        let text = resp.text().await.unwrap_or_default();
        return crate::error_response(
            "api_error",
            &format!("Workers AI REST API error ({status}): {text}"),
            Some(status.into()),
        );
    }

    // Read the SSE stream and convert to Anthropic format
    let stream = build_anthropic_sse_stream(resp, anthropic_model).await?;

    Response::builder()
        .with_status(200)
        .with_header("content-type", "text/event-stream")?
        .with_header("cache-control", "no-cache")?
        .with_header("connection", "keep-alive")?
        .body(Some(Body::from(stream)))
}

/// Read the Workers AI SSE stream and convert to Anthropic SSE events.
async fn build_anthropic_sse_stream(
    mut resp: worker::Response,
    anthropic_model: &str,
) -> Result<String> {
    let mut output = String::new();

    // message_start
    push_event(&mut output, "message_start", &MessageStart {
        event_type: "message_start".into(),
        message: MessageStartMessage {
            id: format!("msg_{}", make_id()),
            msg_type: "message".into(),
            role: "assistant".into(),
            model: anthropic_model.into(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: Usage::default(),
        },
    });

    push_event_str(&mut output, "ping", r#"{"type":"ping"}"#);

    let mut block_index: usize = 0;
    let mut text_started = false;
    let mut finish_reason = "end_turn".to_string();
    let mut active_tool_calls: std::collections::HashMap<u32, ToolState> = std::collections::HashMap::new();

    // Read the response body as text and parse SSE lines
    let body = resp.text().await.unwrap_or_default();

    for line in body.lines() {
        let line = line.trim();
        if line.is_empty() || !line.starts_with("data: ") {
            continue;
        }
        let data = &line[6..];
        if data == "[DONE]" {
            break;
        }

        let chunk: Value = match serde_json::from_str(data) {
            Ok(v) => v,
            Err(_) => continue,
        };

        // Process OpenAI-format delta
        if let Some(choices) = chunk.get("choices").and_then(|c| c.as_array()) {
            if let Some(choice) = choices.first() {
                if let Some(delta) = choice.get("delta") {
                    // ── Text content ──
                    if let Some(text) = delta.get("content").and_then(|c| c.as_str()) {
                        if !text_started {
                            text_started = true;
                            push_event(&mut output, "content_block_start", &ContentBlockStart {
                                event_type: "content_block_start".into(),
                                index: block_index,
                                content_block: json!({ "type": "text", "text": "" }),
                            });
                        }
                        push_event(&mut output, "content_block_delta", &ContentBlockDelta {
                            event_type: "content_block_delta".into(),
                            index: block_index,
                            delta: Delta::TextDelta { text: text.to_string() },
                        });
                    }

                    // ── Tool calls ──
                    if let Some(tcs) = delta.get("tool_calls").and_then(|tc| tc.as_array()) {
                        for tc in tcs {
                            let idx = tc.get("index").and_then(|i| i.as_u64()).unwrap_or(0) as u32;
                            let state = active_tool_calls.entry(idx).or_insert_with(|| ToolState {
                                id: String::new(),
                                name: String::new(),
                                started: false,
                            });

                            if let Some(id) = tc.get("id").and_then(|i| i.as_str()) {
                                state.id = id.to_string();
                            }
                            if let Some(func) = tc.get("function") {
                                if let Some(name) = func.get("name").and_then(|n| n.as_str()) {
                                    state.name = name.to_string();
                                }
                                // Emit content_block_start on first sight
                                if !state.started && !state.name.is_empty() {
                                    state.started = true;
                                    let cb_idx = block_index + idx as usize + if text_started { 1 } else { 0 };
                                    push_event(&mut output, "content_block_start", &ContentBlockStart {
                                        event_type: "content_block_start".into(),
                                        index: cb_idx,
                                        content_block: json!({
                                            "type": "tool_use",
                                            "id": state.id,
                                            "name": state.name,
                                        }),
                                    });
                                }
                                // Emit argument delta
                                if let Some(args) = func.get("arguments").and_then(|a| a.as_str()) {
                                    if !args.is_empty() {
                                        let cb_idx = block_index + idx as usize + if text_started { 1 } else { 0 };
                                        push_event(&mut output, "content_block_delta", &ContentBlockDelta {
                                            event_type: "content_block_delta".into(),
                                            index: cb_idx,
                                            delta: Delta::InputJsonDelta { partial_json: args.to_string() },
                                        });
                                    }
                                }
                            }
                        }
                    }

                    // ── Finish reason ──
                    if let Some(reason) = choice.get("finish_reason").and_then(|r| r.as_str()) {
                        finish_reason = match reason {
                            "stop" => "end_turn",
                            "tool_calls" | "function_call" => "tool_use",
                            "length" => "max_tokens",
                            other => other,
                        }.to_string();
                    }
                }
            }
        }

        // ── Native format: response field ──
        if let Some(resp_text) = chunk.get("response").and_then(|r| r.as_str()) {
            if !resp_text.is_empty() && !text_started {
                text_started = true;
                push_event(&mut output, "content_block_start", &ContentBlockStart {
                    event_type: "content_block_start".into(),
                    index: block_index,
                    content_block: json!({ "type": "text", "text": "" }),
                });
            }
            if !resp_text.is_empty() {
                push_event(&mut output, "content_block_delta", &ContentBlockDelta {
                    event_type: "content_block_delta".into(),
                    index: block_index,
                    delta: Delta::TextDelta { text: resp_text.to_string() },
                });
            }
        }

        // ── Usage ──
        if let Some(usage) = chunk.get("usage") {
            let output_tokens = usage.get("completion_tokens").and_then(|t| t.as_u64()).unwrap_or(0) as u32;
            push_event(&mut output, "message_delta", &MessageDelta {
                event_type: "message_delta".into(),
                delta: MessageDeltaDelta {
                    stop_reason: Some(finish_reason.clone()),
                    stop_sequence: None,
                },
                usage: OutputUsage { output_tokens },
            });
        }
    }

    // Close text block
    if text_started {
        push_event(&mut output, "content_block_stop", &ContentBlockStop {
            event_type: "content_block_stop".into(),
            index: block_index,
        });
    }

    // Close tool call blocks
    let tool_offset = if text_started { 1 } else { 0 };
    let mut sorted_indices: Vec<u32> = active_tool_calls.keys().copied().collect();
    sorted_indices.sort();
    for idx in &sorted_indices {
        let cb_idx = block_index + *idx as usize + tool_offset;
        push_event(&mut output, "content_block_stop", &ContentBlockStop {
            event_type: "content_block_stop".into(),
            index: cb_idx,
        });
    }

    push_event_str(&mut output, "message_stop", r#"{"type":"message_stop"}"#);

    Ok(output)
}

/// Fallback: non-streaming binding response sent as a single-chunk SSE stream.
async fn binding_streaming_response(result: &Value, anthropic_model: &str) -> Result<Response> {
    let events = crate::convert::to_anthropic_response(anthropic_model, result);

    let mut output = String::new();

    push_event(&mut output, "message_start", &MessageStart {
        event_type: "message_start".into(),
        message: MessageStartMessage {
            id: events.id.clone(),
            msg_type: "message".into(),
            role: "assistant".into(),
            model: events.model.clone(),
            content: vec![],
            stop_reason: None,
            stop_sequence: None,
            usage: Usage::default(),
        },
    });

    push_event_str(&mut output, "ping", r#"{"type":"ping"}"#);

    let mut block_index = 0;
    for block in &events.content {
        match block {
            ContentBlock::Text { text } => {
                push_event(&mut output, "content_block_start", &ContentBlockStart {
                    event_type: "content_block_start".into(),
                    index: block_index,
                    content_block: json!({ "type": "text", "text": "" }),
                });
                push_event(&mut output, "content_block_delta", &ContentBlockDelta {
                    event_type: "content_block_delta".into(),
                    index: block_index,
                    delta: Delta::TextDelta { text: text.clone() },
                });
                push_event(&mut output, "content_block_stop", &ContentBlockStop {
                    event_type: "content_block_stop".into(),
                    index: block_index,
                });
                block_index += 1;
            }
            ContentBlock::ToolUse { id, name, input } => {
                push_event(&mut output, "content_block_start", &ContentBlockStart {
                    event_type: "content_block_start".into(),
                    index: block_index,
                    content_block: json!({
                        "type": "tool_use",
                        "id": id,
                        "name": name,
                    }),
                });
                push_event(&mut output, "content_block_delta", &ContentBlockDelta {
                    event_type: "content_block_delta".into(),
                    index: block_index,
                    delta: Delta::InputJsonDelta {
                        partial_json: serde_json::to_string(input).unwrap_or_default(),
                    },
                });
                push_event(&mut output, "content_block_stop", &ContentBlockStop {
                    event_type: "content_block_stop".into(),
                    index: block_index,
                });
                block_index += 1;
            }
            ContentBlock::Thinking { .. } => {}
        }
    }

    push_event(&mut output, "message_delta", &MessageDelta {
        event_type: "message_delta".into(),
        delta: MessageDeltaDelta {
            stop_reason: events.stop_reason.clone(),
            stop_sequence: None,
        },
        usage: OutputUsage {
            output_tokens: events.usage.output_tokens,
        },
    });

    push_event_str(&mut output, "message_stop", r#"{"type":"message_stop"}"#);

    Response::builder()
        .with_status(200)
        .with_header("content-type", "text/event-stream")?
        .with_header("cache-control", "no-cache")?
        .body(Some(Body::from(output)))
}

// ── Helpers ───────────────────────────────────────────────────────

fn push_event(output: &mut String, name: &str, data: &impl serde::Serialize) {
    let json = serde_json::to_string(data).unwrap_or_default();
    output.push_str(&format!("event: {name}\ndata: {json}\n\n"));
}

fn push_event_str(output: &mut String, name: &str, data: &str) {
    output.push_str(&format!("event: {name}\ndata: {data}\n\n"));
}

fn make_id() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    format!("{}{:06}", t.as_secs(), t.subsec_micros())
}

struct ToolState {
    id: String,
    name: String,
    started: bool,
}

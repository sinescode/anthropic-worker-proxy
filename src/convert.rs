use crate::types::*;
use serde_json::{json, Value};

/// Convert an Anthropic request into Workers AI input format (OpenAI-compatible).
pub fn to_workers_input(req: &AnthropicRequest) -> Result<Value, String> {
    let mut messages = Vec::new();

    // System prompt → system message
    if let Some(system) = &req.system {
        messages.push(json!({
            "role": "system",
            "content": system.extract_text()
        }));
    }

    for msg in &req.messages {
        let converted = convert_message(msg)?;
        for m in converted {
            messages.push(m);
        }
    }

    let mut input = json!({
        "messages": messages,
        "max_tokens": req.max_tokens.unwrap_or(4096),
    });

    if let Some(t) = req.temperature {
        input["temperature"] = json!(t);
    }
    if let Some(tp) = req.top_p {
        input["top_p"] = json!(tp);
    }
    if let Some(stop) = &req.stop_sequences {
        input["stop"] = json!(stop);
    }

    // Tools
    if let Some(tools) = &req.tools {
        let mapped: Vec<Value> = tools
            .iter()
            .map(|t| {
                json!({
                    "type": "function",
                    "function": {
                        "name": t.name,
                        "description": t.description,
                        "parameters": t.input_schema
                    }
                })
            })
            .collect();
        input["tools"] = json!(mapped);
    }

    // Tool choice
    if let Some(tc) = &req.tool_choice {
        let disable_parallel = match tc {
            ToolChoice::Auto { disable_parallel_tool_use } => *disable_parallel_tool_use,
            ToolChoice::Any { disable_parallel_tool_use } => *disable_parallel_tool_use,
            ToolChoice::Tool { disable_parallel_tool_use, .. } => *disable_parallel_tool_use,
            ToolChoice::None => None,
        };
        if disable_parallel.unwrap_or(false) {
            input["parallel_tool_calls"] = json!(false);
        }

        input["tool_choice"] = match tc {
            ToolChoice::Auto { .. } => json!("auto"),
            ToolChoice::Any { .. } => json!("required"),
            ToolChoice::Tool { name, .. } => {
                json!({ "type": "function", "function": { "name": name } })
            }
            ToolChoice::None => json!("none"),
        };
    }

    Ok(input)
}

/// Convert a single Anthropic message into one or more Workers AI messages.
fn convert_message(msg: &Message) -> Result<Vec<Value>, String> {
    let mut out = Vec::new();

    match &msg.content {
        Content::Text(text) => {
            out.push(json!({ "role": msg.role, "content": text }));
        }
        Content::Blocks(blocks) => {
            if msg.content.is_tool_result() {
                // Tool results → individual tool messages
                for tr in msg.content.tool_results() {
                    let text = extract_tool_result_text(&tr.content);
                    let content = if tr.is_error.unwrap_or(false) {
                        format!("[ERROR] {text}")
                    } else {
                        text
                    };
                    out.push(json!({
                        "role": "tool",
                        "tool_call_id": tr.tool_use_id,
                        "content": content
                    }));
                }
            } else if msg.content.is_tool_use() && msg.role == "assistant" {
                // Assistant tool calls → OpenAI tool_calls format
                let text = msg.content.extract_text();
                let tool_uses = msg.content.tool_uses();

                let tool_calls: Vec<Value> = tool_uses
                    .iter()
                    .map(|tu| {
                        json!({
                            "id": tu.id,
                            "type": "function",
                            "function": {
                                "name": tu.name,
                                "arguments": serde_json::to_string(&tu.input).unwrap_or_default()
                            }
                        })
                    })
                    .collect();

                let mut m = json!({ "role": "assistant", "content": text });
                if !tool_calls.is_empty() {
                    m["tool_calls"] = json!(tool_calls);
                }
                out.push(m);
            } else {
                // Regular content blocks
                let images = msg.content.extract_images();
                let text = msg.content.extract_text();

                if !images.is_empty() {
                    let mut content_array = Vec::new();
                    if !text.is_empty() {
                        content_array.push(json!({ "type": "text", "text": text }));
                    }
                    for img in &images {
                        if let Some(data) = &img.data {
                            let mt = img.media_type.as_deref().unwrap_or("image/png");
                            content_array.push(json!({
                                "type": "image_url",
                                "image_url": { "url": format!("data:{mt};base64,{data}") }
                            }));
                        } else if let Some(url) = &img.url {
                            content_array.push(json!({
                                "type": "image_url",
                                "image_url": { "url": url }
                            }));
                        }
                    }
                    out.push(json!({ "role": msg.role, "content": content_array }));
                } else {
                    out.push(json!({ "role": msg.role, "content": text }));
                }
            }
        }
    }

    Ok(out)
}

/// Convert a Workers AI response (OpenAI format) to Anthropic format.
pub fn to_anthropic_response(model: &str, result: &Value) -> Response {
    let content = extract_content_blocks(result);
    let stop_reason = extract_stop_reason(result);
    let usage = extract_usage(result);

    Response {
        id: format!("msg_{}", crate::make_id()),
        response_type: "message".into(),
        role: "assistant".into(),
        model: model.into(),
        content,
        stop_reason,
        stop_sequence: None,
        usage,
    }
}

// ── Extraction helpers ────────────────────────────────────────────

fn extract_content_blocks(result: &Value) -> Vec<ContentBlock> {
    let mut blocks = Vec::new();

    // OpenAI format: choices[0].message
    if let Some(message) = result
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("message"))
    {
        // Text
        if let Some(text) = message.get("content").and_then(|c| c.as_str()) {
            if !text.is_empty() {
                blocks.push(ContentBlock::Text { text: text.to_string() });
            }
        }

        // Tool calls
        if let Some(tcs) = message.get("tool_calls").and_then(|tc| tc.as_array()) {
            for tc in tcs {
                if let Some(func) = tc.get("function") {
                    let name = func.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string();
                    let args_str = func.get("arguments").and_then(|a| a.as_str()).unwrap_or("{}");
                    let input: Value = serde_json::from_str(args_str).unwrap_or(json!({}));
                    blocks.push(ContentBlock::ToolUse {
                        id: tc.get("id").and_then(|i| i.as_str()).unwrap_or("").to_string(),
                        name,
                        input,
                    });
                }
            }
        }
    }

    // Native format fallback
    if blocks.is_empty() {
        if let Some(resp) = result.get("response") {
            let text = if let Some(s) = resp.as_str() {
                s.to_string()
            } else {
                resp.to_string()
            };
            blocks.push(ContentBlock::Text { text });
        }
    }

    if blocks.is_empty() {
        blocks.push(ContentBlock::Text { text: String::new() });
    }

    blocks
}

fn extract_stop_reason(result: &Value) -> Option<String> {
    result
        .get("choices")
        .and_then(|c| c.as_array())
        .and_then(|a| a.first())
        .and_then(|c| c.get("finish_reason"))
        .and_then(|r| r.as_str())
        .map(|r| match r {
            "stop" => "end_turn",
            "tool_calls" | "function_call" => "tool_use",
            "length" => "max_tokens",
            _ => "end_turn",
        })
        .map(String::from)
        .or_else(|| Some("end_turn".to_string()))
}

fn extract_usage(result: &Value) -> Usage {
    let u = result.get("usage");
    Usage {
        input_tokens: u.and_then(|u| u.get("prompt_tokens")).and_then(|t| t.as_u64()).unwrap_or(0) as u32,
        output_tokens: u.and_then(|u| u.get("completion_tokens")).and_then(|t| t.as_u64()).unwrap_or(0) as u32,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: u
            .and_then(|u| u.get("prompt_tokens_details"))
            .and_then(|d| d.get("cached_tokens"))
            .and_then(|t| t.as_u64())
            .map(|t| t as u32),
    }
}

fn extract_tool_result_text(content: &Option<Value>) -> String {
    match content {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join(""),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

mod config;
mod convert;
mod stream;
mod types;

use config::*;
use types::*;
use worker::*;

#[event(fetch)]
async fn main(req: Request, env: Env, _ctx: Context) -> Result<Response> {
    // CORS preflight
    if req.method() == Method::Options {
        return cors_response(204);
    }

    let mut resp = match req.url()?.path() {
        "/v1/messages" if req.method() == Method::Post => handle_messages(req, env).await,
        "/v1/models" => handle_models(),
        "/" | "/health" => health_check(&env),
        _ => error_response("not_found", "Endpoint not found", Some(404)),
    };

    // Attach CORS headers to all responses
    if let Ok(ref mut r) = resp {
        for (key, value) in cors_headers() {
            r.headers_mut().set(key, value)?;
        }
    }

    resp
}

// ══════════════════════════════════════════════════════════════════
// Handlers
// ══════════════════════════════════════════════════════════════════

async fn handle_messages(mut req: Request, env: Env) -> Result<Response> {
    // Auth check
    let api_key = req.headers().get("x-api-key")?.unwrap_or_default();
    if api_key.is_empty() {
        return error_response("authentication_error", "Missing x-api-key header", Some(401));
    }

    // Check for model override via cf-model header
    let cf_model_override = req
        .headers()
        .get("cf-model")?
        .and_then(|v| if v.is_empty() { None } else { Some(v) });

    // Parse body
    let body: AnthropicRequest = match req.json().await {
        Ok(b) => b,
        Err(e) => return error_response("invalid_request_error", &format!("Invalid JSON body: {e}"), Some(400)),
    };

    // Validate
    if body.messages.is_empty() {
        return error_response("invalid_request_error", "messages array must not be empty", Some(400));
    }
    if let Some(max) = body.max_tokens {
        if max == 0 {
            return error_response("invalid_request_error", "max_tokens must be > 0", Some(400));
        }
    }

    // Resolve model: cf-model header > env var > built-in mapping
    let workers_model = if let Some(custom) = &cf_model_override {
        // Custom model from header — use as-is
        custom.clone()
    } else {
        let model_map = ModelMap::from_env();
        model_map.resolve(&body.model)
    };

    // Convert request
    let workers_input = match convert::to_workers_input(&body) {
        Ok(input) => input,
        Err(e) => return error_response("invalid_request_error", &e, Some(400)),
    };

    // Dispatch
    if body.stream.unwrap_or(false) {
        stream::handle_streaming(&body, &workers_model, &workers_input, &env).await
    } else {
        let ai: Ai = env.ai("AI")?;
        let result: serde_json::Value = match ai
            .run(&workers_model, workers_input, AiOptions::default())
            .await
        {
            Ok(r) => r,
            Err(e) => {
                return error_response(
                    "api_error",
                    &format!("Workers AI inference failed: {e}"),
                    Some(502),
                );
            }
        };

        let response = convert::to_anthropic_response(&body.model, &result);
        Ok(Response::from_json(&response)?)
    }
}

fn handle_models() -> Result<Response> {
    let model_map = ModelMap::from_env();

    // Built-in Anthropic models → Workers AI mapping (generated from defaults)
    let anthropic_models: Vec<serde_json::Value> = model_map
        .known_models()
        .into_iter()
        .map(|m| {
            let workers = model_map.resolve(m);
            serde_json::json!({
                "id": m,
                "object": "model",
                "owned_by": "anthropic-worker-proxy",
                "workers_ai_model": workers
            })
        })
        .collect();

    // Popular Workers AI models (usable via cf-model header)
    let workers_models = vec![
        ("@cf/meta/llama-3.1-8b-instruct", "Llama 3.1 8B — fast, good for chat"),
        ("@cf/meta/llama-3.3-70b-instruct-fp8-fast", "Llama 3.3 70B — best free model"),
        ("@cf/meta/llama-4-scout-17b-16e-instruct", "Llama 4 Scout 17B — latest Llama"),
        ("@cf/qwen/qwen2.5-coder-32b-instruct", "Qwen 2.5 Coder 32B — best for code"),
        ("@cf/qwen/qwq-32b", "Qwen QwQ 32B — reasoning model"),
        ("@cf/zai-org/glm-4.7-flash", "GLM 4.7 Flash — fast, multilingual"),
        ("@cf/moonshotai/kimi-k2.7-code", "Kimi K2.7 Code — 256k context, reasoning"),
        ("@cf/openai/gpt-oss-120b", "GPT-OSS 120B — OpenAI open-weights"),
        ("@cf/baai/bge-base-en-v1.5", "BGE Embeddings — for embedMany"),
        ("@cf/black-forest-labs/flux-1-schnell", "FLUX Schnell — image generation"),
    ]
    .into_iter()
    .map(|(id, desc)| {
        serde_json::json!({
            "id": id,
            "description": desc,
            "usage": "Pass as cf-model header or set MODEL_CLAUDE_SONNET_4_5 env var"
        })
    })
    .collect::<Vec<_>>();

    Response::from_json(&serde_json::json!({
        "anthropic_models": anthropic_models,
        "workers_ai_models": workers_models,
        "how_to_use": {
            "method_1_header": "curl -H 'cf-model: @cf/zai-org/glm-4.7-flash' ...",
            "method_2_env_var": "export MODEL_CLAUDE_SONNET_4_5='@cf/zai-org/glm-4.7-flash'",
            "method_3_direct": "curl -d '{\"model\": \"@cf/zai-org/glm-4.7-flash\"}' ..."
        }
    }))
}

// ══════════════════════════════════════════════════════════════════
// Helpers
// ══════════════════════════════════════════════════════════════════

fn health_check(env: &Env) -> Result<Response> {
    let has_account_id = env.var("CLOUDFLARE_ACCOUNT_ID").ok().map(|v| !v.to_string().is_empty()).unwrap_or(false);
    let has_api_token = env.var("CLOUDFLARE_API_TOKEN").ok().map(|v| !v.to_string().is_empty()).unwrap_or(false);

    Response::from_json(&serde_json::json!({
        "status": "ok",
        "service": "anthropic-worker-proxy",
        "version": "0.3.0",
        "streaming": {
            "enabled": has_account_id && has_api_token,
            "mode": if has_account_id && has_api_token { "token-by-token SSE" } else { "single-chunk fallback" },
            "configured": {
                "cloudflare_account_id": has_account_id,
                "cloudflare_api_token": has_api_token
            },
            "setup_help": if !has_account_id || !has_api_token {
                Some("To enable token-by-token streaming, run: wrangler secret put CLOUDFLARE_ACCOUNT_ID && wrangler secret put CLOUDFLARE_API_TOKEN")
            } else {
                None
            }
        }
    }))
}

fn cors_response(status: u16) -> Result<Response> {
    let mut resp = Response::builder().with_status(status).empty()?;
    for (key, value) in cors_headers() {
        resp.headers_mut().set(key, value)?;
    }
    Ok(resp)
}

pub(crate) fn error_response(error_type: &str, message: &str, status: Option<u16>) -> Result<Response> {
    let body = anthropic_error(error_type, message);
    let s = status.unwrap_or(400);
    Response::builder()
        .with_status(s)
        .with_header("content-type", "application/json")?
        .body(Some(Body::from(serde_json::to_string(&body).unwrap_or_default())))
}

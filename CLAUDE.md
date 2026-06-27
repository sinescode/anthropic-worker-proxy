# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project

A Cloudflare Worker (Rust → WASM) that proxies the [Anthropic Messages API](https://docs.anthropic.com/en/api/messages) to [Workers AI](https://developers.cloudflare.com/workers-ai/), translating between Anthropic and OpenAI chat-completion formats on the fly. Primary use case: running Claude Code against free Workers AI models.

## Commands

```bash
# Local dev server (port 8787)
wrangler dev

# Deploy to Cloudflare
wrangler deploy

# Set streaming credentials (needed for true token-by-token SSE)
wrangler secret put CLOUDFLARE_ACCOUNT_ID
wrangler secret put CLOUDFLARE_API_TOKEN
```

**Do NOT run `cargo` commands locally** (build, test, check, clippy, etc.). Compilation happens inside `wrangler dev`/`wrangler deploy`. No local test suite exists — test manually with curl against the dev server.

## Architecture

```
Claude Code → [Anthropic format] → POST /v1/messages → [OpenAI format] → Workers AI
                                        ↑                                    ↓
                                   GET /v1/models                    Ai binding or REST API
                                   GET /health                             ↓
Claude Code ← [Anthropic format] ← SSE/JSON   ←── [OpenAI format] ← Response
```

**Request flow (`lib.rs:42-101`):**
1. Auth check via `x-api-key` header (any non-empty value passes)
2. `cf-model` header checked for per-request model override
3. Body parsed as `AnthropicRequest`, validated (non-empty messages, max_tokens > 0)
4. Model resolved: `cf-model` header > `MODEL_*` env var > built-in defaults > `@cf/` prefix guess
5. Request converted to OpenAI format via `convert::to_workers_input()`
6. Dispatched: streaming → `stream::handle_streaming()`, non-streaming → `env.AI.run()`
7. Response converted back to Anthropic format via `convert::to_anthropic_response()`

### Source modules

| File | Purpose |
|---|---|
| `lib.rs` | Worker entry point (`#[event(fetch)]`), router, auth, orchestration |
| `types.rs` | Anthropic request/response structs, streaming SSE event types, error types |
| `convert.rs` | Anthropic ↔ OpenAI format bridging: messages, tools, images, responses |
| `config.rs` | `ModelMap` (env-var-driven model routing), CORS headers, error formatting |
| `stream.rs` | SSE streaming: REST API path (true token-by-token) and Ai binding fallback (single chunk) |

### Key design decisions

- **Dual streaming strategy**: When `CLOUDFLARE_ACCOUNT_ID` + `CLOUDFLARE_API_TOKEN` secrets are set, uses `fetch()` to the Cloudflare REST API for raw SSE. Otherwise falls back to the typed `Ai` binding which deserializes the response (losing the stream — all tokens delivered at once as a synthetic SSE stream).
- **Model resolution priority** (`config.rs`): `cf-model` header → `MODEL_CLAUDE_SONNET_4_5` env var → built-in default (`@cf/meta/llama-3.3-70b-instruct-fp8-fast`). Env var names are derived from the Anthropic model name uppercased with underscores (e.g., `claude-sonnet-4-5` → `MODEL_CLAUDE_SONNET_4_5`).
- **Format bridge** (`convert.rs`): System prompts → system message, tool definitions → OpenAI `tools[]` format, tool use → `tool_calls[]` with JSON-stringified arguments, images → `image_url` with base64 data URIs, tool results → `tool` role messages. Responses map `finish_reason` (stop/tool_calls/length) → Anthropic `stop_reason` (end_turn/tool_use/max_tokens).
- **CORS**: All responses get permissive CORS headers (`*` origin, GET/POST/OPTIONS methods). Applied in `lib.rs` after handler dispatch so error responses are also CORS-enabled.
- **Anthropic error format**: All errors use `{"type":"error","error":{"type":"...","message":"..."}}` matching the Anthropic API shape.

### Anthropic streaming event sequence

`message_start` → `ping` → `content_block_start` → `content_block_delta` (×N) → `content_block_stop` → `message_delta` (with usage) → `message_stop`

Tool calls interleave additional `content_block_start`/`delta`/`stop` events with index tracking (`stream.rs:108-206`).

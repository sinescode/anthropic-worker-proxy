# Anthropic Worker Proxy (Rust)

A Cloudflare Worker written in **Rust** that proxies [Anthropic Messages API](https://docs.anthropic.com/en/api/messages) requests to [Workers AI](https://developers.cloudflare.com/workers-ai/).

Drop-in replacement for Claude Code's API backend.

## Features

- **True token-by-token streaming** via REST API
- **Full Anthropic Messages API** format conversion
- **Tool use / function calling** with proper format bridging
- **Image inputs** (vision) with base64 encoding
- **System prompts**, temperature, top_p, stop_sequences
- **CORS support** for browser-based clients
- **Configurable model mapping** via environment variables
- **Proper Anthropic error responses** (auth, validation, API errors)
- **Health check** endpoint at `/`

## Quick Start

```bash
cd anthropic-worker-proxy
wrangler login
wrangler deploy
```

## Usage with Claude Code

```bash
export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.<your-subdomain>.workers.dev
export ANTHROPIC_API_KEY=any-string
claude
```

## Streaming

For **true token-by-token streaming**, set these env vars in `wrangler.toml` or via `wrangler secret`:

```bash
wrangler secret put CLOUDFLARE_ACCOUNT_ID
wrangler secret put CLOUDFLARE_API_TOKEN
```

Without these, streaming falls back to single-chunk mode (all tokens at once).

## Model Mapping

| Anthropic Model | Workers AI Model |
|---|---|
| `claude-sonnet-4-5` | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` |
| `claude-haiku-4-5` | `@cf/meta/llama-3.1-8b-instruct-fast` |
| `claude-opus-4-5` | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` |
| `claude-sonnet-4-6` | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` |
| `claude-opus-4-6` | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` |
| `claude-fable-5` | `@cf/meta/llama-3.1-8b-instruct-fast` |

Dated variants (e.g. `claude-sonnet-4-5-20250929`) are also mapped.
See `/v1/models` for the full list.

### Custom model mapping via env vars

```toml
[vars]
MODEL_CLAUDE_SONNET_4_5 = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
MODEL_CLAUDE_HAIKU_4_5 = "@cf/qwen/qwq-32b"
DEFAULT_MODEL = "@cf/meta/llama-3.3-70b-instruct-fp8-fast"
```

Or pass `@cf/...` model IDs directly as the `model` parameter.

## API Endpoints

| Method | Path | Description |
|---|---|---|
| `POST` | `/v1/messages` | Anthropic Messages API |
| `GET` | `/v1/models` | List available models |
| `GET` | `/` or `/health` | Health check |
| `OPTIONS` | `*` | CORS preflight |

## Error Responses

All errors follow the Anthropic error format:

```json
{
  "type": "error",
  "error": {
    "type": "invalid_request_error",
    "message": "messages array must not be empty"
  }
}
```

Error types: `authentication_error`, `invalid_request_error`, `not_found`, `api_error`, `rate_limit_error`.

## Local Development

```bash
wrangler dev
# Test:
curl http://localhost:8787/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: test" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Say hello in 5 words"}]
  }'
```

## Architecture

```
Claude Code → [Anthropic format] → Proxy Worker → [OpenAI format] → Workers AI
                                  ↓                                    ↓
                           CORS headers                    binding.run() or REST API
                                  ↓                                    ↓
Claude Code ← [Anthropic format] ← Response ←── [OpenAI format] ← Response
```

- **Non-streaming**: Uses `env.AI` binding directly (fast, no auth needed)
- **Streaming**: Uses Cloudflare REST API for true SSE (requires `CLOUDFLARE_ACCOUNT_ID` + `CLOUDFLARE_API_TOKEN`)

## License

MIT

# How to Use Workers AI with Claude Code

Step-by-step guide. Every command, every output, no guessing.

---

## What You Need

| Requirement | Why |
|---|---|
| [Cloudflare account](https://dash.cloudflare.com/sign-up) (free) | To deploy the worker |
| [Node.js](https://nodejs.org/) v18+ | For the Wrangler deploy tool |
| [Rust](https://rustup.rs/) | Wrangler compiles Rust to WebAssembly |
| [Claude Code](https://docs.anthropic.com/en/docs/claude-code) | The CLI you want to use |

---

## Step 1: Deploy the Worker

### 1.1 Open a terminal and go to the project folder

```bash
cd /path/to/anthropic-worker-proxy
```

Your folder should look like this:

```
anthropic-worker-proxy/
├── Cargo.toml
├── wrangler.toml
├── src/
│   ├── lib.rs
│   ├── types.rs
│   ├── convert.rs
│   ├── config.rs
│   └── stream.rs
└── ...
```

### 1.2 Install Wrangler (Cloudflare's deploy tool)

```bash
npm install -g wrangler
```

Verify it installed:

```bash
wrangler --version
# Should print: wrangler 4.x.x
```

### 1.3 Login to Cloudflare

```bash
wrangler login
```

This opens your browser. Click **Authorize** to give Wrangler access to your Cloudflare account.

If the browser doesn't open, copy the URL Wrangler prints and open it manually.

### 1.4 Deploy

```bash
wrangler deploy
```

**What you'll see on first deploy:**

```
 ⛅️ wrangler 4.x.x
 ─────────────────────────────────
 Successfully logged in.
 Compiling Rust to WebAssembly...
 ...
 Uploaded anthropic-worker-proxy (1.xx MB)
 Published https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev
```

**The URL at the end is your proxy URL.** Copy it.

> **Where does the subdomain come from?** It's your Cloudflare account name. You can find it at https://dash.cloudflare.com → any Workers URL you've deployed before, or the right sidebar under "Account ID".

---

## Step 2: Enable Streaming (Optional)

Without this, responses arrive all at once. With it, you see tokens appear one by one (like normal Claude).

### 2.1 Find your Cloudflare Account ID

1. Go to https://dash.cloudflare.com
2. On the **right sidebar**, you'll see **Account ID** — a 32-character string like `abc123def456...`
3. Copy it

### 2.2 Create an API Token

1. Go to https://dash.cloudflare.com/profile/api-tokens
2. Click **Create Token**
3. Click **Use template** next to **Workers Scripts**
4. Click **Continue to summary**
5. Click **Create Token**
6. **Copy the token immediately** (you can't see it again)

### 2.3 Add the secrets to your worker

```bash
wrangler secret put CLOUDFLARE_ACCOUNT_ID
# Paste your Account ID and press Enter

wrangler secret put CLOUDFLARE_API_TOKEN
# Paste your API Token and press Enter
```

You should see `✨ Successfully set secret` after each one.

---

## Step 3: Connect Claude Code

### 3.1 Set environment variables

**Replace `YOUR_SUBDOMAIN` with the one from Step 1.4:**

```bash
export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev
export ANTHROPIC_API_KEY=sk-ant-placeholder
```

### 3.2 Make it permanent

**Bash** — add to `~/.bashrc`:
```bash
echo 'export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev' >> ~/.bashrc
echo 'export ANTHROPIC_API_KEY=sk-ant-placeholder' >> ~/.bashrc
source ~/.bashrc
```

**Zsh** — add to `~/.zshrc`:
```bash
echo 'export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev' >> ~/.zshrc
echo 'export ANTHROPIC_API_KEY=sk-ant-placeholder' >> ~/.zshrc
source ~/.zshrc
```

### 3.3 Verify it works

```bash
curl $ANTHROPIC_BASE_URL/health
```

**Expected:**
```json
{"status":"ok","service":"anthropic-worker-proxy","version":"0.3.0"}
```

---

## Step 4: Run Claude Code

```bash
claude
```

Claude Code now runs on Workers AI.

---

## Using Any Workers AI Model (GLM, Qwen, Kimi, etc.)

**This is the key feature.** You can use ANY Workers AI model with Claude Code — not just the defaults.

### All Available Models

Browse the full catalog: https://developers.cloudflare.com/workers-ai/models/

Popular models:

| Model | ID | Best for |
|---|---|---|
| Llama 3.3 70B | `@cf/meta/llama-3.3-70b-instruct-fp8-fast` | General (default) |
| Llama 3.1 8B | `@cf/meta/llama-3.1-8b-instruct` | Fast, lightweight |
| GLM 4.7 Flash | `@cf/zai-org/glm-4.7-flash` | Fast, multilingual |
| Kimi K2.7 Code | `@cf/moonshotai/kimi-k2.7-code` | Code, 256k context, reasoning |
| Qwen 2.5 Coder 32B | `@cf/qwen/qwen2.5-coder-32b-instruct` | Best for code |
| QwQ 32B | `@cf/qwen/qwq-32b` | Reasoning |
| GPT-OSS 120B | `@cf/openai/gpt-oss-120b` | High reasoning |
| Llama 4 Scout | `@cf/meta/llama-4-scout-17b-16e-instruct` | Latest Llama |

### Method 1: `cf-model` header (per-request)

Pass the `cf-model` header to override the model for any single request:

**With curl:**
```bash
curl $ANTHROPIC_BASE_URL/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: sk-ant-placeholder" \
  -H "anthropic-version: 2023-06-01" \
  -H "cf-model: @cf/zai-org/glm-4.7-flash" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "你好，用中文回答：什么是量子计算？"}]
  }'
```

The `model` field in the body can be any Anthropic model name — the `cf-model` header overrides what actually runs on Workers AI.

**With Claude Code** — you can't add headers directly, but you can use the env var method below.

### Method 2: Environment variable (all requests)

Set which Workers AI model Claude Code uses for all requests:

```bash
# Use GLM for everything
export MODEL_CLAUDE_SONNET_4_5="@cf/zai-org/glm-4.7-flash"
export MODEL_CLAUDE_HAIKU_4_5="@cf/zai-org/glm-4.7-flash"
export MODEL_CLAUDE_OPUS_4_5="@cf/zai-org/glm-4.7-flash"
```

Then run Claude Code:
```bash
claude
```

Now Claude Code sends `claude-sonnet-4-5` to the proxy, but the proxy routes it to GLM.

**Make it permanent:**

```bash
echo 'export MODEL_CLAUDE_SONNET_4_5="@cf/zai-org/glm-4.7-flash"' >> ~/.bashrc
echo 'export MODEL_CLAUDE_HAIKU_4_5="@cf/zai-org/glm-4.7-flash"' >> ~/.bashrc
echo 'export MODEL_CLAUDE_OPUS_4_5="@cf/zai-org/glm-4.7-flash"' >> ~/.bashrc
source ~/.bashrc
```

### Method 3: Edit wrangler.toml (server-side)

Edit `wrangler.toml` on the deployed worker:

```toml
[vars]
MODEL_CLAUDE_SONNET_4_5 = "@cf/zai-org/glm-4.7-flash"
MODEL_CLAUDE_HAIKU_4_5 = "@cf/zai-org/glm-4.7-flash"
MODEL_CLAUDE_OPUS_4_5 = "@cf/zai-org/glm-4.7-flash"
```

Then redeploy:
```bash
wrangler deploy
```

This changes the model for ALL users of the proxy (no env vars needed on client side).

### Method 4: Pass `@cf/...` directly

If you're calling the API directly (not through Claude Code), pass the Workers AI model ID as the `model` field:

```bash
curl $ANTHROPIC_BASE_URL/v1/models
```

This shows all available models with their IDs. Pick one and use it:

```bash
curl $ANTHROPIC_BASE_URL/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: sk-ant-placeholder" \
  -H "anthropic-version: 2023-06-01" \
  -d '{
    "model": "@cf/zai-org/glm-4.7-flash",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "Hello in Chinese"}]
  }'
```

---

## Quick Reference: Use GLM with Claude Code

```bash
# 1. Deploy (one time)
cd anthropic-worker-proxy && wrangler deploy

# 2. Configure Claude Code to use GLM
export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev
export ANTHROPIC_API_KEY=sk-ant-placeholder
export MODEL_CLAUDE_SONNET_4_5="@cf/zai-org/glm-4.7-flash"
export MODEL_CLAUDE_HAIKU_4_5="@cf/zai-org/glm-4.7-flash"
export MODEL_CLAUDE_OPUS_4_5="@cf/zai-org/glm-4.7-flash"

# 3. Run
claude
```

Now every message Claude Code sends goes to GLM 4.7 Flash on Workers AI.

---

## Testing

### Test with GLM

```bash
curl $ANTHROPIC_BASE_URL/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: test" \
  -H "anthropic-version: 2023-06-01" \
  -H "cf-model: @cf/zai-org/glm-4.7-flash" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 100,
    "messages": [{"role": "user", "content": "What is Rust?"}]
  }'
```

### Test with Kimi (code + reasoning)

```bash
curl $ANTHROPIC_BASE_URL/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: test" \
  -H "anthropic-version: 2023-06-01" \
  -H "cf-model: @cf/moonshotai/kimi-k2.7-code" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 200,
    "messages": [{"role": "user", "content": "Write a Rust function to reverse a linked list"}]
  }'
```

### List all models

```bash
curl $ANTHROPIC_BASE_URL/v1/models
```

### Test streaming with GLM

```bash
curl $ANTHROPIC_BASE_URL/v1/messages \
  -H "content-type: application/json" \
  -H "x-api-key: test" \
  -H "anthropic-version: 2023-06-01" \
  -H "cf-model: @cf/zai-org/glm-4.7-flash" \
  -d '{
    "model": "claude-sonnet-4-5",
    "max_tokens": 100,
    "stream": true,
    "messages": [{"role": "user", "content": "Write a haiku about coding"}]
  }'
```

---

## How Model Selection Works

```
Claude Code sends:     { model: "claude-sonnet-4-5", ... }
                              ↓
Proxy checks:
  1. cf-model header? → Use that model
  2. MODEL_CLAUDE_SONNET_4_5 env var? → Use that
  3. Default mapping → @cf/meta/llama-3.3-70b-instruct-fp8-fast
                              ↓
Workers AI runs:       @cf/zai-org/glm-4.7-flash
```

Priority: **header > env var > default**.

---

## Troubleshooting

### "model not found" error

The Workers AI model ID is wrong. Check:
- Model IDs start with `@cf/`
- Browse valid IDs: https://developers.cloudflare.com/workers-ai/models/

### GLM responds in wrong language

GLM supports Chinese natively. Ask in Chinese to get Chinese responses:
```
"用中文解释量子计算"
```

### Want to switch models on the fly?

Use the `cf-model` header with curl:
```bash
# Try GLM
curl -H "cf-model: @cf/zai-org/glm-4.7-flash" ...

# Try Kimi
curl -H "cf-model: @cf/moonshotai/kimi-k2.7-code" ...

# Try Qwen
curl -H "cf-model: @cf/qwen/qwen2.5-coder-32b-instruct" ...
```

### Claude Code shows "claude-sonnet-4-5" but I want GLM

That's normal. Claude Code always shows the Anthropic model name. The proxy silently routes to whatever you configured. The actual model running is on Workers AI.

---

## Cost

| Component | Cost |
|---|---|
| Cloudflare Workers (the proxy) | Free (100K requests/day) |
| Workers AI inference | Free tier: 10,000 neurons/day |
| **Total** | **$0** for normal usage |

---

## Quick Reference Card

```bash
# Deploy
wrangler deploy

# Configure
export ANTHROPIC_BASE_URL=https://anthropic-worker-proxy.YOUR_SUBDOMAIN.workers.dev
export ANTHROPIC_API_KEY=sk-ant-placeholder

# Use GLM
export MODEL_CLAUDE_SONNET_4_5="@cf/zai-org/glm-4.7-flash"

# Run
claude
```

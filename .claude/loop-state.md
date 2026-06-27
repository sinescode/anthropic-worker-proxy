# Loop State

## Cycle log (most recent first)

- Cycle 1 — FIX — replace deprecated llama-3.1-8b-instruct with -fast variant — done

## Upgrade backlog

- [ ] server_tool_use and web_search_tool_result content block types not yet in Block enum (Anthropic API — would cause deserialization errors)
- [ ] thinking_delta and signature_delta SSE delta types not yet in Delta enum (Anthropic API streaming)
- [ ] GLM-5.2 (@cf/zai-org/glm-5.2) as default for Sonnet/Opus — better agentic coding model than llama-3.3-70b
- [ ] Kimi K2.6/K2.7-code and Gemma 4 26B not yet in Workers AI models list in /v1/models

## Last upgrade-scan date

- 2026-06-27: checked Anthropic Messages API docs + Workers AI model catalog. Found: GLM-5.2, Kimi K2.6/K2.7, Gemma 4 26B, Qwen3 30B, Nemotron 3 120B as new Workers AI models. Deprecated: llama-3.1-8b-instruct (non-fast). Anthropic API: adaptive thinking, server_tool_use, web_search_tool_result blocks, thinking_delta/signature_delta in SSE.

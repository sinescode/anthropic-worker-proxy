use std::collections::HashMap;

/// Model mapping: Anthropic model name → Workers AI model id.
///
/// Priority:
///   1. Env var `MODEL_<SAFE_NAME>` (e.g. `MODEL_CLAUDE_SONNET_4_5`)
///   2. Env var `DEFAULT_MODEL`
///   3. Built-in defaults
pub struct ModelMap {
    defaults: HashMap<&'static str, &'static str>,
    env_overrides: HashMap<String, String>,
    default_model: String,
}

impl ModelMap {
    pub fn from_env() -> Self {
        let mut defaults = HashMap::new();
        defaults.insert("claude-sonnet-4-5", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-sonnet-4-5-20250929", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-haiku-4-5", "@cf/meta/llama-3.1-8b-instruct");
        defaults.insert("claude-haiku-4-5-20251001", "@cf/meta/llama-3.1-8b-instruct");
        defaults.insert("claude-opus-4-5", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-opus-4-5-20251101", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-sonnet-4-6", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-opus-4-6", "@cf/meta/llama-3.3-70b-instruct-fp8-fast");
        defaults.insert("claude-fable-5", "@cf/meta/llama-3.1-8b-instruct");

        let default_model = std::env::var("DEFAULT_MODEL")
            .unwrap_or_else(|_| "@cf/meta/llama-3.3-70b-instruct-fp8-fast".into());

        let mut env_overrides = HashMap::new();
        // Check for MODEL_<name> env vars
        for (key, value) in std::env::vars() {
            if let Some(name) = key.strip_prefix("MODEL_") {
                let anthropic_name = name.to_lowercase().replace('_', "-");
                env_overrides.insert(anthropic_name, value);
            }
        }

        Self { defaults, env_overrides, default_model }
    }

    pub fn resolve(&self, anthropic_model: &str) -> String {
        // 1. Exact env override
        if let Some(override_model) = self.env_overrides.get(anthropic_model) {
            return override_model.clone();
        }

        // 2. Built-in mapping
        if let Some(mapped) = self.defaults.get(anthropic_model) {
            return mapped.to_string();
        }

        // 3. Already a Workers AI model id
        if anthropic_model.starts_with("@cf/") {
            return anthropic_model.to_string();
        }

        // 4. Fall back to DEFAULT_MODEL
        self.default_model.clone()
    }

    pub fn default_model(&self) -> &str {
        &self.default_model
    }

    /// All known Anthropic model names (keys from the built-in defaults map).
    pub fn known_models(&self) -> Vec<&str> {
        let mut names: Vec<&str> = self.defaults.keys().copied().collect();
        names.sort();
        names
    }
}

/// CORS headers for browser-based clients.
pub fn cors_headers() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Access-Control-Allow-Origin", "*"),
        ("Access-Control-Allow-Methods", "GET, POST, OPTIONS"),
        ("Access-Control-Allow-Headers", "content-type, x-api-key, anthropic-version, anthropic-beta, cf-model"),
        ("Access-Control-Max-Age", "86400"),
    ]
}

/// Build an Anthropic-format error response.
pub fn anthropic_error(error_type: &str, message: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "error",
        "error": {
            "type": error_type,
            "message": message
        }
    })
}

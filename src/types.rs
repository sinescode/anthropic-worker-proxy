use serde::{Deserialize, Serialize};
use serde_json::Value;

// ══════════════════════════════════════════════════════════════════
// Anthropic Request
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize)]
pub struct AnthropicRequest {
    pub model: String,
    pub messages: Vec<Message>,
    #[serde(default)]
    pub system: Option<SystemContent>,
    #[serde(default)]
    pub max_tokens: Option<u32>,
    #[serde(default)]
    pub temperature: Option<f64>,
    #[serde(default)]
    pub top_p: Option<f64>,
    #[serde(default)]
    pub stop_sequences: Option<Vec<String>>,
    #[serde(default)]
    pub stream: Option<bool>,
    #[serde(default)]
    pub tools: Option<Vec<Tool>>,
    #[serde(default)]
    pub tool_choice: Option<ToolChoice>,
    #[serde(default)]
    pub thinking: Option<Value>,
    #[serde(default)]
    pub metadata: Option<Value>,
}

#[derive(Debug, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Content,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Blocks(Vec<Block>),
}

impl Content {
    pub fn extract_text(&self) -> String {
        match self {
            Content::Text(s) => s.clone(),
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join(""),
        }
    }

    pub fn extract_images(&self) -> Vec<ImageSource> {
        match self {
            Content::Text(_) => vec![],
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    Block::Image { source } => Some(source.clone()),
                    _ => None,
                })
                .collect(),
        }
    }

    pub fn is_tool_result(&self) -> bool {
        matches!(self, Content::Blocks(blocks) if blocks.iter().any(|b| matches!(b, Block::ToolResult { .. })))
    }

    pub fn is_tool_use(&self) -> bool {
        matches!(self, Content::Blocks(blocks) if blocks.iter().any(|b| matches!(b, Block::ToolUse { .. } | Block::ServerToolUse { .. })))
    }

    pub fn tool_results(&self) -> Vec<ToolResultBlock> {
        match self {
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    Block::ToolResult { tool_use_id, content, is_error } => Some(ToolResultBlock {
                        tool_use_id: tool_use_id.clone(),
                        content: content.clone(),
                        is_error: *is_error,
                    }),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }

    pub fn extract_web_search_text(&self) -> Option<String> {
        match self {
            Content::Blocks(blocks) => {
                let parts: Vec<String> = blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::WebSearchToolResult { content, url, title } => {
                            let mut text = String::new();
                            if let Some(t) = title { text.push_str(&format!("[{}] ", t)); }
                            if let Some(c) = content {
                                match c {
                                    Value::String(s) => text.push_str(s),
                                    Value::Array(arr) => {
                                        for item in arr {
                                            if let Some(t) = item.get("text").and_then(|v| v.as_str()) {
                                                text.push_str(t);
                                            }
                                        }
                                    }
                                    other => { let _ = other; }
                                }
                            }
                            if let Some(u) = url { text.push_str(&format!("\nSource: {u}")); }
                            if text.is_empty() { None } else { Some(text) }
                        }
                        _ => None,
                    })
                    .collect();
                if parts.is_empty() { None } else { Some(parts.join("\n\n")) }
            }
            _ => None,
        }
    }

    pub fn tool_uses(&self) -> Vec<ToolUseBlock> {
        match self {
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| match b {
                    Block::ToolUse { id, name, input }
                    | Block::ServerToolUse { id, name, input } => Some(ToolUseBlock {
                        id: id.clone(),
                        name: name.clone(),
                        input: input.clone(),
                    }),
                    _ => None,
                })
                .collect(),
            _ => vec![],
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum Block {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image { source: ImageSource },
    #[serde(rename = "tool_use")]
    ToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        tool_use_id: String,
        #[serde(default)]
        content: Option<Value>,
        #[serde(default)]
        is_error: Option<bool>,
    },
    #[serde(rename = "thinking")]
    Thinking { thinking: String, #[serde(default)] signature: Option<String> },
    #[serde(rename = "redacted_thinking")]
    RedactedThinking { data: String },
    #[serde(rename = "server_tool_use")]
    ServerToolUse {
        id: String,
        name: String,
        input: Value,
    },
    #[serde(rename = "web_search_tool_result")]
    WebSearchToolResult {
        #[serde(default)]
        content: Option<Value>,
        #[serde(default)]
        url: Option<String>,
        #[serde(default)]
        title: Option<String>,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageSource {
    #[serde(rename = "type")]
    pub source_type: String,
    #[serde(default)]
    pub media_type: Option<String>,
    #[serde(default)]
    pub data: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

pub struct ToolResultBlock {
    pub tool_use_id: String,
    pub content: Option<Value>,
    pub is_error: Option<bool>,
}

pub struct ToolUseBlock {
    pub id: String,
    pub name: String,
    pub input: Value,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum SystemContent {
    Text(String),
    Blocks(Vec<SystemBlock>),
}

impl SystemContent {
    pub fn extract_text(&self) -> String {
        match self {
            SystemContent::Text(s) => s.clone(),
            SystemContent::Blocks(blocks) => blocks
                .iter()
                .filter_map(|b| b.text.as_deref())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct SystemBlock {
    #[serde(default)]
    pub text: Option<String>,
    #[serde(default)]
    pub cache_control: Option<Value>,
}

// ══════════════════════════════════════════════════════════════════
// Anthropic Tools
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Deserialize, Serialize)]
pub struct Tool {
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "input_schema")]
    pub input_schema: Value,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ToolChoice {
    #[serde(rename = "auto")]
    Auto {
        #[serde(default)]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "any")]
    Any {
        #[serde(default)]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "tool")]
    Tool {
        name: String,
        #[serde(default)]
        disable_parallel_tool_use: Option<bool>,
    },
    #[serde(rename = "none")]
    None,
}

// ══════════════════════════════════════════════════════════════════
// Anthropic Response
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
pub struct Response {
    pub id: String,
    #[serde(rename = "type")]
    pub response_type: String,
    pub role: String,
    pub model: String,
    pub content: Vec<ContentBlock>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_sequence: Option<Value>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "tool_use")]
    ToolUse { id: String, name: String, input: Value },
    #[serde(rename = "thinking")]
    Thinking { thinking: String },
}

#[derive(Debug, Serialize, Default)]
pub struct Usage {
    pub input_tokens: u32,
    pub output_tokens: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_creation_input_tokens: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cache_read_input_tokens: Option<u32>,
}

// ══════════════════════════════════════════════════════════════════
// Streaming Events
// ══════════════════════════════════════════════════════════════════

#[derive(Debug, Serialize)]
pub struct MessageStart {
    #[serde(rename = "type")]
    pub event_type: String,
    pub message: MessageStartMessage,
}

#[derive(Debug, Serialize)]
pub struct MessageStartMessage {
    pub id: String,
    #[serde(rename = "type")]
    pub msg_type: String,
    pub role: String,
    pub model: String,
    pub content: Vec<Value>,
    pub stop_reason: Option<Value>,
    pub stop_sequence: Option<Value>,
    pub usage: Usage,
}

#[derive(Debug, Serialize)]
pub struct ContentBlockStart {
    #[serde(rename = "type")]
    pub event_type: String,
    pub index: usize,
    pub content_block: Value,
}

#[derive(Debug, Serialize)]
pub struct ContentBlockDelta {
    #[serde(rename = "type")]
    pub event_type: String,
    pub index: usize,
    pub delta: Delta,
}

#[derive(Debug, Serialize)]
#[serde(tag = "type")]
pub enum Delta {
    #[serde(rename = "text_delta")]
    TextDelta { text: String },
    #[serde(rename = "input_json_delta")]
    InputJsonDelta { partial_json: String },
}

#[derive(Debug, Serialize)]
pub struct ContentBlockStop {
    #[serde(rename = "type")]
    pub event_type: String,
    pub index: usize,
}

#[derive(Debug, Serialize)]
pub struct MessageDelta {
    #[serde(rename = "type")]
    pub event_type: String,
    pub delta: MessageDeltaDelta,
    pub usage: OutputUsage,
}

#[derive(Debug, Serialize)]
pub struct MessageDeltaDelta {
    pub stop_reason: Option<String>,
    pub stop_sequence: Option<Value>,
}

#[derive(Debug, Serialize)]
pub struct OutputUsage {
    pub output_tokens: u32,
}


//! One LLM contract for the app: providers, keys, discovery and chat.
//!
//! The config file is shared across craft apps (`~/.craft/ai_config.json`),
//! so the provider Sandra picks here is the provider everywhere. Keys resolve
//! file first, then provider environment variables, and only presence flags
//! ever cross into logs, dispatch replies or the UI.
//!
//! Transport is `curl.exe`, sync, with the body and the auth header passed
//! through files (never the command line: Windows caps a command line at
//! 32,767 characters and the key would show in the process list). Every
//! failure is a `LlmError`. Nothing here invents reply text.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

/// Provider ids. UI strings, config values and MCP contracts use the id.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LlmProvider {
    Ollama,
    LmStudio,
    OpenAi,
    Anthropic,
    DeepSeek,
    Meta,
    OpenRouter,
    Custom,
    Gemini,
}

impl LlmProvider {
    pub const ALL: [Self; 9] = [
        Self::Ollama,
        Self::LmStudio,
        Self::OpenAi,
        Self::Anthropic,
        Self::DeepSeek,
        Self::Meta,
        Self::OpenRouter,
        Self::Custom,
        Self::Gemini,
    ];

    pub fn id(self) -> &'static str {
        match self {
            LlmProvider::Ollama => "ollama",
            LlmProvider::LmStudio => "lmstudio",
            LlmProvider::OpenAi => "openai",
            LlmProvider::Anthropic => "anthropic",
            LlmProvider::DeepSeek => "deepseek",
            LlmProvider::Meta => "meta",
            LlmProvider::OpenRouter => "openrouter",
            LlmProvider::Custom => "custom",
            LlmProvider::Gemini => "gemini",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            LlmProvider::Ollama => "Ollama (local)",
            LlmProvider::LmStudio => "LM Studio (local)",
            LlmProvider::OpenAi => "OpenAI (cloud)",
            LlmProvider::Anthropic => "Anthropic (cloud)",
            LlmProvider::DeepSeek => "DeepSeek (cloud)",
            LlmProvider::Meta => "Meta Llama API (cloud)",
            LlmProvider::OpenRouter => "OpenRouter (cloud)",
            LlmProvider::Custom => "Custom OpenAI-compatible",
            LlmProvider::Gemini => "Google Gemini (cloud)",
        }
    }

    /// Short aliases resolve to the canonical id, then are stored canonical.
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "ollama" | "local" => Some(LlmProvider::Ollama),
            "lmstudio" | "lms" | "lm-studio" => Some(LlmProvider::LmStudio),
            "openai" | "cloud" | "gpt" => Some(LlmProvider::OpenAi),
            "anthropic" | "claude" => Some(LlmProvider::Anthropic),
            "deepseek" => Some(LlmProvider::DeepSeek),
            "meta" | "llama" => Some(LlmProvider::Meta),
            "openrouter" => Some(LlmProvider::OpenRouter),
            "custom" | "vllm" | "compat" | "openai-compat" | "openaicompat" => {
                Some(LlmProvider::Custom)
            }
            "gemini" | "google" => Some(LlmProvider::Gemini),
            _ => None,
        }
    }

    /// Local providers need no key.
    pub fn local(self) -> bool {
        matches!(self, LlmProvider::Ollama | LlmProvider::LmStudio)
    }

    /// Providers with a live model list endpoint. Anthropic and Gemini use a
    /// curated list plus free text.
    pub fn has_list_api(self) -> bool {
        !matches!(self, LlmProvider::Anthropic | LlmProvider::Gemini)
    }

    /// Environment variable holding this provider's key, if any.
    pub fn env_key(self) -> Option<&'static str> {
        match self {
            LlmProvider::OpenAi => Some("OPENAI_API_KEY"),
            LlmProvider::Anthropic => Some("ANTHROPIC_API_KEY"),
            LlmProvider::DeepSeek => Some("DEEPSEEK_API_KEY"),
            LlmProvider::Meta => Some("META_API_KEY"),
            LlmProvider::OpenRouter => Some("OPENROUTER_API_KEY"),
            LlmProvider::Custom => Some("CUSTOM_API_KEY"),
            LlmProvider::Gemini => Some("GEMINI_API_KEY"),
            _ => None,
        }
    }

    fn default_endpoint(self) -> &'static str {
        match self {
            LlmProvider::Ollama => "http://127.0.0.1:11434",
            LlmProvider::LmStudio => "http://127.0.0.1:1234",
            LlmProvider::OpenAi => "https://api.openai.com",
            LlmProvider::Anthropic => "https://api.anthropic.com",
            LlmProvider::DeepSeek => "https://api.deepseek.com",
            LlmProvider::Meta => "https://api.llama.com/compat/v1",
            LlmProvider::OpenRouter => "https://openrouter.ai/api/v1",
            LlmProvider::Custom => "",
            LlmProvider::Gemini => "https://generativelanguage.googleapis.com",
        }
    }
}

/// Curated model names where no list API exists.
pub const ANTHROPIC_MODELS: &[&str] = &["claude-opus-4-6", "claude-sonnet-4-6", "claude-haiku-4-5"];
pub const GEMINI_MODELS: &[&str] = &["gemini-2.5-flash", "gemini-2.5-pro", "gemini-2.0-flash"];

/// Everything the app needs to talk to a model. No blind defaults: a fresh
/// config has an empty model until discovery picks an installed one.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct LlmConfig {
    pub provider: String,
    pub model: String,
    /// Classifier override for the drive proposal AI pass. Empty means chat.
    pub classify_provider: String,
    pub classify_model: String,
    pub ollama_endpoint: String,
    pub lmstudio_endpoint: String,
    pub custom_endpoint: String,
    pub openai_key: String,
    pub anthropic_key: String,
    pub deepseek_key: String,
    pub meta_key: String,
    pub openrouter_key: String,
    pub custom_key: String,
    pub gemini_key: String,
    pub user_extra_prompt: String,
    pub tools_enabled: bool,
    pub save_transcripts: bool,
}

impl Default for LlmConfig {
    fn default() -> Self {
        LlmConfig {
            provider: "ollama".to_string(),
            model: String::new(),
            classify_provider: String::new(),
            classify_model: String::new(),
            ollama_endpoint: "http://127.0.0.1:11434".to_string(),
            lmstudio_endpoint: "http://127.0.0.1:1234".to_string(),
            custom_endpoint: String::new(),
            openai_key: String::new(),
            anthropic_key: String::new(),
            deepseek_key: String::new(),
            meta_key: String::new(),
            openrouter_key: String::new(),
            custom_key: String::new(),
            gemini_key: String::new(),
            user_extra_prompt: String::new(),
            tools_enabled: true,
            save_transcripts: true,
        }
    }
}

impl LlmConfig {
    pub fn provider_enum(&self) -> LlmProvider {
        LlmProvider::parse(&self.provider).unwrap_or(LlmProvider::Ollama)
    }

    /// Base URL for this provider: explicit custom endpoint first, otherwise
    /// the matching field, otherwise the default.
    pub fn endpoint(&self) -> String {
        let p = self.provider_enum();
        let field = match p {
            LlmProvider::Ollama => self.ollama_endpoint.trim(),
            LlmProvider::LmStudio => self.lmstudio_endpoint.trim(),
            _ => self.custom_endpoint.trim(),
        };
        if !field.is_empty() {
            return field.to_string();
        }
        if matches!(
            p,
            LlmProvider::OpenAi
                | LlmProvider::Anthropic
                | LlmProvider::DeepSeek
                | LlmProvider::Meta
                | LlmProvider::OpenRouter
                | LlmProvider::Custom
                | LlmProvider::Gemini
                | LlmProvider::LmStudio
        ) && p == LlmProvider::LmStudio
        {
            return LlmProvider::LmStudio.default_endpoint().to_string();
        }
        if p == LlmProvider::Ollama {
            return LlmProvider::Ollama.default_endpoint().to_string();
        }
        p.default_endpoint().to_string()
    }

    fn key_slot(&self, provider: LlmProvider) -> &str {
        match provider {
            LlmProvider::OpenAi => &self.openai_key,
            LlmProvider::Anthropic => &self.anthropic_key,
            LlmProvider::DeepSeek => &self.deepseek_key,
            LlmProvider::Meta => &self.meta_key,
            LlmProvider::OpenRouter => &self.openrouter_key,
            LlmProvider::Custom => &self.custom_key,
            LlmProvider::Gemini => &self.gemini_key,
            _ => "",
        }
    }

    /// Resolve the key for a provider: manual entry wins, else the provider
    /// environment variable. Empty means no key.
    pub fn resolve_key(&self, provider: LlmProvider) -> String {
        let manual = self.key_slot(provider).trim();
        if !manual.is_empty() {
            return manual.to_string();
        }
        provider
            .env_key()
            .and_then(|var| std::env::var(var).ok())
            .map(|v| v.trim().to_string())
            .unwrap_or_default()
    }

    /// Presence flags only. A key value must never reach a log or a reply.
    pub fn keys_present(&self) -> Vec<(&'static str, bool)> {
        LlmProvider::ALL
            .iter()
            .filter(|p| !p.local())
            .map(|p| (p.id(), !self.resolve_key(*p).is_empty()))
            .collect()
    }

    /// Guard for send time: a model must be selected and, for cloud
    /// providers, a key must resolve. Otherwise refuse with a Settings pointer.
    pub fn send_ready(&self) -> Result<(), LlmError> {
        if self.model.trim().is_empty() {
            return Err(LlmError::NoModel);
        }
        let p = self.provider_enum();
        if !p.local() && self.resolve_key(p).is_empty() {
            return Err(LlmError::MissingKey {
                provider: p.id().to_string(),
            });
        }
        Ok(())
    }

    /// Effective classifier: explicit override wins, otherwise the chat pair.
    /// Empty model (here and there) refuses at send time, never invents one.
    pub fn classifier(&self) -> (LlmProvider, String) {
        let provider = LlmProvider::parse(self.classify_provider.trim())
            .unwrap_or_else(|| self.provider_enum());
        let model = if self.classify_model.trim().is_empty() {
            self.model.trim().to_string()
        } else {
            self.classify_model.trim().to_string()
        };
        (provider, model)
    }
}

/// Shared config path: one provider selection for every craft app.
pub fn config_path() -> PathBuf {
    let home = std::env::var("USERPROFILE")
        .or_else(|_| std::env::var("HOME"))
        .unwrap_or_else(|_| ".".to_string());
    PathBuf::from(home).join(".craft").join("ai_config.json")
}

pub fn load_config() -> LlmConfig {
    match std::fs::read_to_string(config_path()) {
        Ok(text) => serde_json::from_str::<LlmConfig>(&text).unwrap_or_default(),
        Err(_) => LlmConfig::default(),
    }
}

/// Atomic write: a crash mid-write must not leave half a file.
pub fn save_config(config: &LlmConfig) -> Result<(), String> {
    let path = config_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot create {}: {e}", parent.display()))?;
    }
    let tmp = path.with_extension("json.tmp");
    let body = serde_json::to_string_pretty(config)
        .map_err(|e| format!("cannot serialise LLM config: {e}"))?;
    std::fs::write(&tmp, body).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("cannot move {}: {e}", path.display()))?;
    Ok(())
}

/// What went wrong. The key is never part of any message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LlmError {
    /// No model selected. Pick one in Settings.
    NoModel,
    /// A cloud provider without a resolvable key.
    MissingKey { provider: String },
    /// curl.exe is not on PATH.
    CurlMissing,
    /// The server could not be reached at all.
    Unreachable { detail: String },
    /// HTTP error status with a truncated body.
    Http { status: i64, body: String },
    /// The reply did not parse, or the model returned no content.
    BadReply { detail: String },
    /// The user cancelled.
    Cancelled,
}

impl std::fmt::Display for LlmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LlmError::NoModel => write!(f, "no model selected: pick one in Settings"),
            LlmError::MissingKey { provider } => {
                write!(f, "no API key for {provider}: enter one in Settings or set its environment variable")
            }
            LlmError::CurlMissing => write!(f, "curl.exe is not on PATH"),
            LlmError::Unreachable { detail } => write!(f, "server unreachable: {detail}"),
            LlmError::Http { status, body } => write!(f, "HTTP {status}: {body}"),
            LlmError::BadReply { detail } => write!(f, "unusable reply: {detail}"),
            LlmError::Cancelled => write!(f, "cancelled"),
        }
    }
}

// ---------------------------------------------------------------------------
// Discovery
// ---------------------------------------------------------------------------

/// One installed model found by live probing.
#[derive(Debug, Clone)]
pub struct DiscoveredModel {
    pub name: String,
    pub size_gb: Option<f64>,
    pub resident: bool,
}

/// Run curl and split the trailing HTTP status line off stdout.
/// `connect_timeout_s` caps the stall before a server is called offline.
fn curl_get(url: &str, connect_timeout_s: u64, max_time_s: u64) -> Result<(i64, String), LlmError> {
    let out = std::process::Command::new("curl.exe")
        .args([
            "-sS",
            "-X",
            "GET",
            url,
            "--connect-timeout",
            &connect_timeout_s.to_string(),
            "--max-time",
            &max_time_s.to_string(),
            "-w",
            "\n%{http_code}",
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LlmError::CurlMissing
            } else {
                LlmError::Unreachable {
                    detail: e.to_string(),
                }
            }
        })?;
    let text = String::from_utf8_lossy(&out.stdout).to_string();
    let mut parts = text.rsplit('\n');
    let code: i64 = parts.next().unwrap_or("").trim().parse().unwrap_or(0);
    let body = parts
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<Vec<_>>()
        .join("\n");
    Ok((code, body))
}

fn truncate_middle(body: &str, cap: usize) -> String {
    if body.len() <= cap {
        return body.to_string();
    }
    format!("{}...", &body[..cap.min(body.len())])
}

/// Live list for Ollama (`/api/tags` plus `/api/ps` residents, never fakes).
pub fn probe_ollama(config: &LlmConfig) -> Result<Vec<DiscoveredModel>, LlmError> {
    let endpoint = if config.ollama_endpoint.trim().is_empty() {
        LlmProvider::Ollama.default_endpoint().to_string()
    } else {
        config.ollama_endpoint.trim().to_string()
    };
    let (code, body) = curl_get(&format!("{endpoint}/api/tags"), 2, 10)?;
    if code != 200 {
        return Err(LlmError::Http {
            status: code,
            body: truncate_middle(body.trim(), 300),
        });
    }
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| LlmError::BadReply {
        detail: format!("Ollama tags did not parse: {e}"),
    })?;
    let residents: std::collections::HashSet<String> =
        curl_get(&format!("{endpoint}/api/ps"), 2, 10)
            .ok()
            .and_then(|(c, b)| {
                if c == 200 {
                    serde_json::from_str::<serde_json::Value>(&b).ok()
                } else {
                    None
                }
            })
            .and_then(|v| v.get("models")?.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .filter_map(|m| m.get("name")?.as_str().map(str::to_string))
            .collect();
    let mut models = Vec::new();
    if let Some(list) = value.get("models").and_then(|m| m.as_array()) {
        for m in list {
            let Some(name) = m.get("name").and_then(|n| n.as_str()) else {
                continue;
            };
            let size_gb = m
                .get("size")
                .and_then(serde_json::Value::as_u64)
                .map(|b| (b as f64) / 1_000_000_000.0);
            models.push(DiscoveredModel {
                name: name.to_string(),
                size_gb,
                resident: residents.contains(name),
            });
        }
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

/// Live list for any OpenAI-compatible `/v1/models` endpoint (LM Studio,
/// OpenAI, DeepSeek, Meta, OpenRouter, custom). Short timeouts: an offline
/// server must never stall the UI.
pub fn probe_openai_models(base_url: &str) -> Result<Vec<DiscoveredModel>, LlmError> {
    let base = base_url.trim().trim_end_matches('/');
    if base.is_empty() {
        return Err(LlmError::Unreachable {
            detail: "no endpoint configured".to_string(),
        });
    }
    let (code, body) = curl_get(&format!("{base}/v1/models"), 2, 10)?;
    if code != 200 {
        return Err(LlmError::Http {
            status: code,
            body: truncate_middle(body.trim(), 300),
        });
    }
    let value: serde_json::Value = serde_json::from_str(&body).map_err(|e| LlmError::BadReply {
        detail: format!("model list did not parse: {e}"),
    })?;
    let mut models = Vec::new();
    if let Some(list) = value.get("data").and_then(|d| d.as_array()) {
        for m in list {
            if let Some(id) = m.get("id").and_then(|i| i.as_str()) {
                models.push(DiscoveredModel {
                    name: id.to_string(),
                    size_gb: None,
                    resident: false,
                });
            }
        }
    }
    models.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(models)
}

// ---------------------------------------------------------------------------
// Chat
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatMessage {
    pub role: ChatRole,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_call_id: Option<String>,
}

impl ChatMessage {
    pub fn user(content: impl Into<String>) -> Self {
        ChatMessage {
            role: ChatRole::User,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }

    pub fn system(content: impl Into<String>) -> Self {
        ChatMessage {
            role: ChatRole::System,
            content: content.into(),
            tool_calls: Vec::new(),
            tool_call_id: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

#[derive(Debug, Clone)]
pub struct ChatRequest {
    pub messages: Vec<ChatMessage>,
    pub tools: Vec<ToolSpec>,
    pub max_tokens: u32,
    pub num_ctx: Option<u32>,
}

#[derive(Debug, Clone)]
pub enum ChatReply {
    Text(String),
    ToolCalls(Vec<ToolCall>),
}

#[derive(Debug, Clone)]
pub struct ChatOutcome {
    pub reply: ChatReply,
    pub prompt_tokens: Option<u64>,
    pub completion_tokens: Option<u64>,
}

/// Cooperative cancel flag. The transport polls it while curl runs and kills
/// the child on cancel, so the UI thread never waits on the network.
#[derive(Debug, Clone, Default)]
pub struct Cancel {
    flag: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl Cancel {
    pub fn new() -> Self {
        Cancel::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, std::sync::atomic::Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.flag.load(std::sync::atomic::Ordering::SeqCst)
    }
}

fn openai_function(spec: &ToolSpec) -> serde_json::Value {
    serde_json::json!({
        "type": "function",
        "function": {
            "name": spec.name,
            "description": spec.description,
            "parameters": spec.input_schema,
        }
    })
}

/// Build the provider request body. Pure: unit-tested without a network.
fn build_body(
    provider: LlmProvider,
    config: &LlmConfig,
    req: &ChatRequest,
) -> Result<(String, serde_json::Value), LlmError> {
    let model = config.model.trim();
    if model.is_empty() {
        return Err(LlmError::NoModel);
    }
    let tools: Vec<serde_json::Value> = req.tools.iter().map(openai_function).collect();
    let with_tools = !tools.is_empty();
    match provider {
        LlmProvider::Ollama => {
            let endpoint = if config.ollama_endpoint.trim().is_empty() {
                LlmProvider::Ollama.default_endpoint().to_string()
            } else {
                config.ollama_endpoint.trim().to_string()
            };
            let mut messages = Vec::new();
            for m in &req.messages {
                let role = match m.role {
                    ChatRole::System => "system",
                    ChatRole::User => "user",
                    ChatRole::Assistant => "assistant",
                    ChatRole::Tool => "tool",
                };
                messages.push(serde_json::json!({"role": role, "content": m.content}));
            }
            let mut body = serde_json::json!({
                "model": model,
                "messages": messages,
                "stream": false,
                "think": false,
            });
            if let Some(n) = req.num_ctx {
                body["options"] = serde_json::json!({"num_ctx": n});
            }
            if with_tools {
                body["tools"] = serde_json::Value::Array(tools);
            }
            Ok((format!("{endpoint}/api/chat"), body))
        }
        LlmProvider::LmStudio
        | LlmProvider::OpenAi
        | LlmProvider::DeepSeek
        | LlmProvider::Meta
        | LlmProvider::OpenRouter
        | LlmProvider::Custom => {
            let base = config.endpoint();
            let mut messages = Vec::new();
            for m in &req.messages {
                match m.role {
                    ChatRole::Tool => messages.push(serde_json::json!({
                        "role": "tool",
                        "tool_call_id": m.tool_call_id.clone().unwrap_or_default(),
                        "content": m.content,
                    })),
                    ChatRole::Assistant if !m.tool_calls.is_empty() => {
                        let calls: Vec<serde_json::Value> = m
                            .tool_calls
                            .iter()
                            .map(|c| {
                                serde_json::json!({
                                    "id": c.id,
                                    "type": "function",
                                    "function": {
                                        "name": c.name,
                                        "arguments": c.arguments.to_string(),
                                    }
                                })
                            })
                            .collect();
                        messages.push(serde_json::json!({
                            "role": "assistant",
                            "content": m.content,
                            "tool_calls": calls,
                        }));
                    }
                    _ => {
                        let role = match m.role {
                            ChatRole::System => "system",
                            ChatRole::User => "user",
                            _ => "assistant",
                        };
                        messages.push(serde_json::json!({"role": role, "content": m.content}));
                    }
                }
            }
            let mut body = serde_json::json!({
                "model": model,
                "messages": messages,
                "max_tokens": req.max_tokens,
            });
            if with_tools {
                body["tools"] = serde_json::Value::Array(tools);
            }
            Ok((
                format!("{}/v1/chat/completions", base.trim_end_matches('/')),
                body,
            ))
        }
        LlmProvider::Anthropic => {
            let mut system = String::new();
            let mut messages = Vec::new();
            for m in &req.messages {
                match m.role {
                    ChatRole::System => {
                        if !system.is_empty() {
                            system.push('\n');
                        }
                        system.push_str(&m.content);
                    }
                    ChatRole::Tool => messages.push(serde_json::json!({
                        "role": "user",
                        "content": [{
                            "type": "tool_result",
                            "tool_use_id": m.tool_call_id.clone().unwrap_or_default(),
                            "content": m.content,
                        }]
                    })),
                    ChatRole::Assistant if !m.tool_calls.is_empty() => {
                        let mut blocks = Vec::new();
                        if !m.content.is_empty() {
                            blocks.push(serde_json::json!({"type": "text", "text": m.content}));
                        }
                        for c in &m.tool_calls {
                            blocks.push(serde_json::json!({
                                "type": "tool_use",
                                "id": c.id,
                                "name": c.name,
                                "input": c.arguments,
                            }));
                        }
                        messages.push(serde_json::json!({"role": "assistant", "content": blocks}));
                    }
                    _ => {
                        let role = if matches!(m.role, ChatRole::User) {
                            "user"
                        } else {
                            "assistant"
                        };
                        messages.push(serde_json::json!({"role": role, "content": m.content}));
                    }
                }
            }
            let tools: Vec<serde_json::Value> = req
                .tools
                .iter()
                .map(|t| {
                    serde_json::json!({
                        "name": t.name,
                        "description": t.description,
                        "input_schema": t.input_schema,
                    })
                })
                .collect();
            let mut body = serde_json::json!({
                "model": model,
                "max_tokens": req.max_tokens,
                "messages": messages,
            });
            if !system.is_empty() {
                body["system"] = serde_json::Value::String(system);
            }
            if !tools.is_empty() {
                body["tools"] = serde_json::Value::Array(tools);
            }
            Ok(("https://api.anthropic.com/v1/messages".to_string(), body))
        }
        LlmProvider::Gemini => {
            let mut system_text = String::new();
            let mut contents = Vec::new();
            for m in &req.messages {
                match m.role {
                    ChatRole::System => {
                        if !system_text.is_empty() {
                            system_text.push('\n');
                        }
                        system_text.push_str(&m.content);
                    }
                    ChatRole::Tool => contents.push(serde_json::json!({
                        "role": "function",
                        "parts": [{
                            "functionResponse": {
                                "name": m.tool_call_id.clone().unwrap_or_default(),
                                "response": {"result": m.content}
                            }
                        }]
                    })),
                    ChatRole::Assistant if !m.tool_calls.is_empty() => {
                        let mut parts = Vec::new();
                        if !m.content.is_empty() {
                            parts.push(serde_json::json!({"text": m.content}));
                        }
                        for c in &m.tool_calls {
                            parts.push(serde_json::json!({
                                "functionCall": {"name": c.name, "args": c.arguments}
                            }));
                        }
                        contents.push(serde_json::json!({"role": "model", "parts": parts}));
                    }
                    _ => {
                        let role = if matches!(m.role, ChatRole::User) {
                            "user"
                        } else {
                            "model"
                        };
                        contents.push(
                            serde_json::json!({"role": role, "parts": [{"text": m.content}]}),
                        );
                    }
                }
            }
            let mut body = serde_json::json!({"contents": contents});
            if !system_text.is_empty() {
                body["system_instruction"] = serde_json::json!({"parts": [{"text": system_text}]});
            }
            if !req.tools.is_empty() {
                let decls: Vec<serde_json::Value> = req
                    .tools
                    .iter()
                    .map(|t| {
                        serde_json::json!({
                            "name": t.name,
                            "description": t.description,
                            "parameters": t.input_schema,
                        })
                    })
                    .collect();
                body["tools"] = serde_json::json!([{"functionDeclarations": decls}]);
            }
            Ok((
                "https://generativelanguage.googleapis.com/v1beta/models/".to_string()
                    + model
                    + ":generateContent",
                body,
            ))
        }
    }
}

fn oauth_header(provider: LlmProvider, key: &str) -> Option<String> {
    if key.is_empty() {
        return None;
    }
    Some(match provider {
        LlmProvider::Anthropic => format!("x-api-key: {key}"),
        LlmProvider::Gemini => format!("x-goog-api-key: {key}"),
        _ => format!("Authorization: Bearer {key}"),
    })
}

/// Parse one provider reply. Pure: unit-tested on recorded JSON. Empty
/// content with no tool calls is an error, never invented text.
fn parse_reply(provider: LlmProvider, value: &serde_json::Value) -> Result<ChatOutcome, LlmError> {
    let bad = |why: &str| LlmError::BadReply {
        detail: why.to_string(),
    };
    let usage = |obj: Option<&serde_json::Value>| -> (Option<u64>, Option<u64>) {
        let prompt = obj
            .and_then(|u| u.get("prompt_tokens").or_else(|| u.get("input_tokens")))
            .and_then(serde_json::Value::as_u64);
        let completion = obj
            .and_then(|u| {
                u.get("completion_tokens")
                    .or_else(|| u.get("output_tokens"))
            })
            .and_then(serde_json::Value::as_u64);
        (prompt, completion)
    };
    match provider {
        LlmProvider::Ollama => {
            let message = value
                .get("message")
                .ok_or_else(|| bad("no message object"))?;
            let (prompt, completion) = usage(Some(value));
            let mut calls = Vec::new();
            if let Some(list) = message.get("tool_calls").and_then(|t| t.as_array()) {
                for c in list {
                    let function = c.get("function").unwrap_or(c);
                    let name = function
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| bad("tool call without a name"))?;
                    let arguments = function
                        .get("arguments")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null);
                    let arguments = match arguments {
                        serde_json::Value::String(s) => {
                            serde_json::from_str(&s).unwrap_or(serde_json::Value::String(s))
                        }
                        other => other,
                    };
                    calls.push(ToolCall {
                        id: c
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: name.to_string(),
                        arguments,
                    });
                }
            }
            if calls.is_empty() {
                let text = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .trim();
                if text.is_empty() {
                    return Err(bad("empty content and no tool calls"));
                }
                Ok(ChatOutcome {
                    reply: ChatReply::Text(text.to_string()),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            } else {
                Ok(ChatOutcome {
                    reply: ChatReply::ToolCalls(calls),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            }
        }
        LlmProvider::LmStudio
        | LlmProvider::OpenAi
        | LlmProvider::DeepSeek
        | LlmProvider::Meta
        | LlmProvider::OpenRouter
        | LlmProvider::Custom => {
            let message = value
                .pointer("/choices/0/message")
                .ok_or_else(|| bad("no choices[0].message"))?;
            let (prompt, completion) = usage(value.get("usage"));
            let mut calls = Vec::new();
            if let Some(list) = message.get("tool_calls").and_then(|t| t.as_array()) {
                for c in list {
                    let function = c
                        .get("function")
                        .ok_or_else(|| bad("tool call without function"))?;
                    let name = function
                        .get("name")
                        .and_then(|n| n.as_str())
                        .ok_or_else(|| bad("tool call without a name"))?;
                    let raw = function
                        .get("arguments")
                        .and_then(|a| a.as_str())
                        .unwrap_or("null");
                    let arguments: serde_json::Value = serde_json::from_str(raw)
                        .map_err(|_| bad("tool call arguments are not JSON"))?;
                    calls.push(ToolCall {
                        id: c
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: name.to_string(),
                        arguments,
                    });
                }
            }
            if calls.is_empty() {
                let text = message
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .trim();
                if text.is_empty() {
                    return Err(bad("empty content and no tool calls"));
                }
                Ok(ChatOutcome {
                    reply: ChatReply::Text(text.to_string()),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            } else {
                Ok(ChatOutcome {
                    reply: ChatReply::ToolCalls(calls),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            }
        }
        LlmProvider::Anthropic => {
            let blocks = value
                .pointer("/content")
                .and_then(|c| c.as_array())
                .ok_or_else(|| bad("no content blocks"))?;
            let (prompt, completion) = usage(value.get("usage"));
            let mut text = String::new();
            let mut calls = Vec::new();
            for b in blocks {
                match b.get("type").and_then(|t| t.as_str()).unwrap_or("") {
                    "text" => {
                        if let Some(t) = b.get("text").and_then(|t| t.as_str()) {
                            if !text.is_empty() {
                                text.push('\n');
                            }
                            text.push_str(t);
                        }
                    }
                    "tool_use" => calls.push(ToolCall {
                        id: b
                            .get("id")
                            .and_then(|i| i.as_str())
                            .unwrap_or("")
                            .to_string(),
                        name: b
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                        arguments: b.get("input").cloned().unwrap_or(serde_json::Value::Null),
                    }),
                    _ => {}
                }
            }
            if calls.is_empty() {
                if text.trim().is_empty() {
                    return Err(bad("empty content and no tool calls"));
                }
                Ok(ChatOutcome {
                    reply: ChatReply::Text(text),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            } else {
                Ok(ChatOutcome {
                    reply: ChatReply::ToolCalls(calls),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            }
        }
        LlmProvider::Gemini => {
            let parts = value
                .pointer("/candidates/0/content/parts")
                .and_then(|p| p.as_array())
                .ok_or_else(|| bad("no candidates[0].content.parts"))?;
            let (prompt, completion) = usage(value.get("usageMetadata"));
            let mut text = String::new();
            let mut calls = Vec::new();
            for p in parts {
                if let Some(t) = p.get("text").and_then(|t| t.as_str()) {
                    if !text.is_empty() {
                        text.push('\n');
                    }
                    text.push_str(t);
                }
                if let Some(call) = p.get("functionCall") {
                    calls.push(ToolCall {
                        id: String::new(),
                        name: call
                            .get("name")
                            .and_then(|n| n.as_str())
                            .unwrap_or("")
                            .to_string(),
                        arguments: call.get("args").cloned().unwrap_or(serde_json::Value::Null),
                    });
                }
            }
            if calls.is_empty() {
                if text.trim().is_empty() {
                    return Err(bad("empty content and no tool calls"));
                }
                Ok(ChatOutcome {
                    reply: ChatReply::Text(text),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            } else {
                Ok(ChatOutcome {
                    reply: ChatReply::ToolCalls(calls),
                    prompt_tokens: prompt,
                    completion_tokens: completion,
                })
            }
        }
    }
}

/// One chat turn: build, send, parse. Sync and cancellable; the UI runs it on
/// a worker thread. The body and the auth header travel through files, never
/// the command line.
pub fn chat(
    config: &LlmConfig,
    req: &ChatRequest,
    cancel: &Cancel,
) -> Result<ChatOutcome, LlmError> {
    config.send_ready()?;
    if cancel.is_cancelled() {
        return Err(LlmError::Cancelled);
    }
    let provider = config.provider_enum();
    let (url, body) = build_body(provider, config, req)?;
    let key = config.resolve_key(provider);
    if !provider.local() && key.is_empty() {
        return Err(LlmError::MissingKey {
            provider: provider.id().to_string(),
        });
    }
    let dir = std::env::temp_dir().join(format!("magpiecraft-llm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| LlmError::Unreachable {
        detail: format!("cannot stage request: {e}"),
    })?;
    let body_file = dir.join("body.json");
    let header_file = dir.join("header.txt");
    std::fs::write(
        &body_file,
        serde_json::to_string(&body).map_err(|e| LlmError::BadReply {
            detail: format!("cannot serialise request: {e}"),
        })?,
    )
    .map_err(|e| LlmError::Unreachable {
        detail: format!("cannot stage request: {e}"),
    })?;
    let mut child = std::process::Command::new("curl.exe");
    child
        .arg("-sS")
        .arg("-X")
        .arg("POST")
        .arg(&url)
        .arg("-H")
        .arg("Content-Type: application/json")
        .arg("--data-binary")
        .arg(format!("@{}", body_file.display()))
        .arg("--connect-timeout")
        .arg("5")
        .arg("--max-time")
        .arg("120")
        .arg("-w")
        .arg("\n%{http_code}");
    if provider == LlmProvider::Anthropic {
        child.arg("--header").arg("anthropic-version: 2023-06-01");
    }
    if provider == LlmProvider::OpenRouter {
        child
            .arg("--header")
            .arg("HTTP-Referer: https://github.com/sandraschi/magpiecraft");
    }
    if let Some(header) = oauth_header(provider, &key) {
        std::fs::write(&header_file, header).map_err(|e| LlmError::Unreachable {
            detail: format!("cannot stage request: {e}"),
        })?;
        child
            .arg("--header")
            .arg(format!("@{}", header_file.display()));
    }
    if provider == LlmProvider::Ollama {
        child.arg("--header").arg("Accept: application/json");
    }
    let mut child = child
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LlmError::CurlMissing
            } else {
                LlmError::Unreachable {
                    detail: e.to_string(),
                }
            }
        })?;
    loop {
        if cancel.is_cancelled() {
            let _ = child.kill();
            let _ = std::fs::remove_dir_all(&dir);
            return Err(LlmError::Cancelled);
        }
        match child.try_wait().map_err(|e| LlmError::Unreachable {
            detail: e.to_string(),
        })? {
            Some(status) => {
                let mut stdout = String::new();
                if let Some(mut pipe) = child.stdout.take() {
                    use std::io::Read as _;
                    let _ = pipe.read_to_string(&mut stdout);
                }
                let mut stderr = String::new();
                if let Some(mut pipe) = child.stderr.take() {
                    use std::io::Read as _;
                    let _ = pipe.read_to_string(&mut stderr);
                }
                let _ = std::fs::remove_dir_all(&dir);
                if !status.success() && stdout.trim().is_empty() {
                    return Err(LlmError::Unreachable {
                        detail: truncate_middle(stderr.trim(), 300),
                    });
                }
                let mut parts = stdout.rsplit('\n');
                let code: i64 = parts.next().unwrap_or("").trim().parse().unwrap_or(0);
                let body_text = parts
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<Vec<_>>()
                    .join("\n");
                if code != 200 {
                    return Err(LlmError::Http {
                        status: code,
                        body: truncate_middle(body_text.trim(), 500),
                    });
                }
                let value: serde_json::Value =
                    serde_json::from_str(&body_text).map_err(|e| LlmError::BadReply {
                        detail: format!("reply did not parse: {e}"),
                    })?;
                return parse_reply(provider, &value);
            }
            None => std::thread::sleep(Duration::from_millis(50)),
        }
    }
}

impl From<LlmError> for crate::MagpieError {
    fn from(e: LlmError) -> Self {
        crate::MagpieError::llm(e.to_string())
    }
}

// ---------------------------------------------------------------------------
// Ollama VRAM management and live test
// ---------------------------------------------------------------------------

fn ollama_post(endpoint: &str, payload: &serde_json::Value) -> Result<(), LlmError> {
    let out = std::process::Command::new("curl.exe")
        .args([
            "-sS",
            "-X",
            "POST",
            &format!("{endpoint}/api/generate"),
            "-H",
            "Content-Type: application/json",
            "--data-binary",
            &format!("@{}", stash_payload(payload)?),
            "--connect-timeout",
            "2",
            "--max-time",
            "120",
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                LlmError::CurlMissing
            } else {
                LlmError::Unreachable {
                    detail: e.to_string(),
                }
            }
        })?;
    if out.status.success() {
        Ok(())
    } else {
        Err(LlmError::Unreachable {
            detail: truncate_middle(String::from_utf8_lossy(&out.stderr).trim(), 200),
        })
    }
}

fn stash_payload(payload: &serde_json::Value) -> Result<String, LlmError> {
    let dir = std::env::temp_dir().join(format!("magpiecraft-llm-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| LlmError::Unreachable {
        detail: format!("cannot stage request: {e}"),
    })?;
    let file = dir.join("gen.json");
    std::fs::write(
        &file,
        serde_json::to_string(payload).map_err(|e| LlmError::BadReply {
            detail: format!("cannot serialise request: {e}"),
        })?,
    )
    .map_err(|e| LlmError::Unreachable {
        detail: format!("cannot stage request: {e}"),
    })?;
    Ok(file.display().to_string())
}

/// Evict every resident Ollama model except `keep` (empty keeps none),
/// then warm `keep` with a one-token generate. Saving a selection switches
/// VRAM, never just config.
pub fn switch_ollama_model(config: &LlmConfig, keep: &str) -> Result<serde_json::Value, LlmError> {
    let endpoint = if config.ollama_endpoint.trim().is_empty() {
        LlmProvider::Ollama.default_endpoint().to_string()
    } else {
        config.ollama_endpoint.trim().to_string()
    };
    let residents = curl_get(&format!("{endpoint}/api/ps"), 2, 10)
        .ok()
        .and_then(|(c, b)| {
            if c == 200 {
                serde_json::from_str::<serde_json::Value>(&b).ok()
            } else {
                None
            }
        })
        .and_then(|v| v.get("models")?.as_array().cloned())
        .unwrap_or_default();
    let mut evicted = Vec::new();
    for m in &residents {
        if let Some(name) = m.get("name").and_then(|n| n.as_str()) {
            if name != keep {
                ollama_post(
                    &endpoint,
                    &serde_json::json!({"model": name, "keep_alive": 0}),
                )?;
                evicted.push(name.to_string());
            }
        }
    }
    let warmed = if keep.is_empty() {
        false
    } else {
        ollama_post(
            &endpoint,
            &serde_json::json!({
                "model": keep,
                "prompt": "ok",
                "stream": false,
                "think": false,
                "keep_alive": "30m",
                "options": {"num_predict": 1},
            }),
        )
        .is_ok()
    };
    Ok(serde_json::json!({"evicted": evicted, "warmed": keep, "warm_ok": warmed}))
}

/// Live validation: one short completion through the selected provider.
/// `ok` is true only on a live reply; curated names without a key are an error.
pub fn test_completion(
    config: &LlmConfig,
    prompt: Option<&str>,
) -> Result<serde_json::Value, LlmError> {
    let started = std::time::Instant::now();
    let outcome = chat(
        config,
        &ChatRequest {
            messages: vec![ChatMessage::user(
                prompt.unwrap_or("Reply with exactly: OK"),
            )],
            tools: vec![],
            max_tokens: 32,
            num_ctx: None,
        },
        &Cancel::new(),
    )?;
    let reply = match outcome.reply {
        ChatReply::Text(t) => t,
        ChatReply::ToolCalls(_) => "(tool calls)".to_string(),
    };
    Ok(serde_json::json!({
        "ok": true,
        "ms": started.elapsed().as_millis() as u64,
        "reply": truncate_middle(reply.trim(), 500),
        "prompt_tokens": outcome.prompt_tokens,
        "completion_tokens": outcome.completion_tokens,
    }))
}

/// Model list for the picker: live where an API exists, curated names where
/// it does not. Never an error: failures come back as the second element so
/// the UI shows them next to the picker instead of crashing it.
pub fn discover_models(config: &LlmConfig) -> (Vec<DiscoveredModel>, Option<String>) {
    let provider = config.provider_enum();
    if !provider.has_list_api() {
        let curated = match provider {
            LlmProvider::Anthropic => ANTHROPIC_MODELS,
            _ => GEMINI_MODELS,
        };
        return (
            curated
                .iter()
                .map(|m| DiscoveredModel {
                    name: m.to_string(),
                    size_gb: None,
                    resident: false,
                })
                .collect(),
            None,
        );
    }
    let result = match provider {
        LlmProvider::Ollama => probe_ollama(config).map_err(|e| e.to_string()),
        LlmProvider::LmStudio => {
            probe_openai_models(&config.lmstudio_endpoint).map_err(|e| e.to_string())
        }
        _ => probe_openai_models(&config.endpoint()).map_err(|e| e.to_string()),
    };
    match result {
        Ok(models) => (models, None),
        Err(detail) => (Vec::new(), Some(detail)),
    }
}

/// Parse one model reply for the drive classifier. Never fails the scan:
/// garbage becomes unknown at zero confidence with a note as the reason.
fn parse_classification(text: &str) -> (String, f32, String) {
    let trimmed = text.trim();
    let unfenced = trimmed
        .strip_prefix("```json")
        .and_then(|s| s.strip_suffix("```"))
        .or_else(|| {
            trimmed
                .strip_prefix("```")
                .and_then(|s| s.strip_suffix("```"))
        })
        .unwrap_or(trimmed)
        .trim();
    let start = unfenced.find("{").unwrap_or(0);
    let end = unfenced.rfind("}").map(|i| i + 1).unwrap_or(unfenced.len());
    let value: serde_json::Value =
        serde_json::from_str(&unfenced[start..end]).unwrap_or(serde_json::Value::Null);
    let kind_raw = value
        .get("kind")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .trim()
        .to_ascii_lowercase();
    let known = [
        "comics",
        "manga",
        "magazines",
        "video",
        "audio",
        "ebook",
        "photo",
        "other",
        "unknown",
    ];
    let kind = if known.contains(&kind_raw.as_str()) {
        kind_raw
    } else {
        "unknown".to_string()
    };
    let confidence = value
        .get("confidence")
        .and_then(|v| v.as_f64())
        .map(|c| ((c.clamp(0.0, 1.0) * 100.0).round() / 100.0) as f32)
        .unwrap_or(0.0);
    let reason = value
        .get("reason")
        .and_then(|v| v.as_str())
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .unwrap_or("model gave no reason")
        .to_string();
    (kind, confidence, reason)
}

/// Classify one unknown level-2 folder with the effective classifier model
/// (override or chat pair). Only read-only context goes in: the folder name
/// plus sample file names. Transport errors propagate; unparseable model
/// output degrades to unknown instead of failing the scan.
pub fn classify_folder_ai(
    config: &LlmConfig,
    name: &str,
    samples: &[String],
    cancel: &Cancel,
) -> Result<(String, f32, String), LlmError> {
    let (provider, model) = config.classifier();
    if model.trim().is_empty() {
        return Err(LlmError::NoModel);
    }
    let mut effective = config.clone();
    effective.provider = provider.id().to_string();
    effective.model = model;
    effective.send_ready()?;
    let sample_list = samples
        .iter()
        .take(20)
        .map(|s| s.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let prompt = format!("You catalogue a home media library. Folder name: {name}. Sample files: {sample_list}. Reply with strict JSON only, no prose: {{\"kind\": one of comics, manga, magazines, video, audio, ebook, photo, other, unknown, \"confidence\": 0.0 to 1.0, \"reason\": one short sentence}}.");
    let req = ChatRequest {
        messages: vec![ChatMessage::user(prompt)],
        tools: Vec::new(),
        max_tokens: 128,
        num_ctx: None,
    };
    let outcome = chat(&effective, &req, cancel)?;
    match outcome.reply {
        ChatReply::Text(text) => Ok(parse_classification(&text)),
        ChatReply::ToolCalls(_) => Ok((
            "unknown".to_string(),
            0.0,
            "model called tools instead of answering".to_string(),
        )),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn cfg(provider: &str, model: &str) -> LlmConfig {
        LlmConfig {
            provider: provider.to_string(),
            model: model.to_string(),
            ..LlmConfig::default()
        }
    }

    #[test]
    fn classifier_defaults_to_chat_pair() {
        let config = cfg("ollama", "gemma3");
        assert_eq!(
            config.classifier(),
            (LlmProvider::Ollama, "gemma3".to_string())
        );
        let mut over = config.clone();
        over.classify_provider = "anthropic".to_string();
        over.classify_model = "claude-sonnet-4-6".to_string();
        assert_eq!(
            over.classifier(),
            (LlmProvider::Anthropic, "claude-sonnet-4-6".to_string())
        );
        let mut half = config.clone();
        half.classify_model = "other-model".to_string();
        assert_eq!(
            half.classifier(),
            (LlmProvider::Ollama, "other-model".to_string())
        );
        let mut bad = config.clone();
        bad.classify_provider = "nope".to_string();
        assert_eq!(
            bad.classifier(),
            (LlmProvider::Ollama, "gemma3".to_string())
        );
    }

    #[test]
    fn classification_parsing_never_fails_the_scan() {
        assert_eq!(
            parse_classification(
                r#"{"kind": "video", "confidence": 0.8, "reason": "series in name"}"#
            ),
            ("video".to_string(), 0.8, "series in name".to_string())
        );
        assert_eq!(parse_classification("```json\n{\"kind\": \"manga\", \"confidence\": 0.95, \"reason\": \"nihongo\"}\n```"), ("manga".to_string(), 0.95, "nihongo".to_string()));
        assert_eq!(
            parse_classification(r#"{"kind": "spaceship", "confidence": 2.5}"#),
            (
                "unknown".to_string(),
                1.0,
                "model gave no reason".to_string()
            )
        );
        assert_eq!(
            parse_classification("not json at all"),
            (
                "unknown".to_string(),
                0.0,
                "model gave no reason".to_string()
            )
        );
    }

    #[test]
    fn providers_roundtrip_and_aliases_resolve() {
        for p in LlmProvider::ALL {
            assert_eq!(LlmProvider::parse(p.id()), Some(p));
        }
        assert_eq!(LlmProvider::parse("local"), Some(LlmProvider::Ollama));
        assert_eq!(LlmProvider::parse("claude"), Some(LlmProvider::Anthropic));
        assert_eq!(LlmProvider::parse("llama"), Some(LlmProvider::Meta));
        assert_eq!(LlmProvider::parse("nope"), None);
        assert!(LlmProvider::Ollama.local());
        assert!(!LlmProvider::OpenAi.local());
        assert!(!LlmProvider::Anthropic.has_list_api());
        assert!(LlmProvider::LmStudio.has_list_api());
    }

    #[test]
    fn empty_model_refuses_before_any_network() {
        let config = LlmConfig::default();
        assert!(config.model.is_empty());
        assert!(matches!(
            chat(
                &config,
                &ChatRequest {
                    messages: vec![ChatMessage::user("hi")],
                    tools: vec![],
                    max_tokens: 64,
                    num_ctx: None,
                },
                &Cancel::new()
            ),
            Err(LlmError::NoModel)
        ));
    }

    #[test]
    fn cloud_without_key_refuses_with_provider_name() {
        let config = cfg("anthropic", "claude-sonnet-4-6");
        assert_eq!(
            config.send_ready(),
            Err(LlmError::MissingKey {
                provider: "anthropic".to_string()
            })
        );
    }

    #[test]
    fn key_never_leaves_the_config_in_errors() {
        let mut config = cfg("openai", "gpt-4o-mini");
        config.openai_key = "sk-secret".to_string();
        // Discovery against a dead port must fail without echoing anything.
        let err = probe_openai_models("http://127.0.0.1:9").unwrap_err();
        assert!(!err.to_string().contains("sk-secret"));
    }

    #[test]
    fn ollama_body_sets_stream_false_think_false() {
        let config = cfg("ollama", "gemma4:12b");
        let (url, body) = build_body(
            LlmProvider::Ollama,
            &config,
            &ChatRequest {
                messages: vec![ChatMessage::user("hi")],
                tools: vec![],
                max_tokens: 64,
                num_ctx: Some(16384),
            },
        )
        .unwrap();
        assert!(url.ends_with("/api/chat"));
        assert_eq!(body["stream"], serde_json::json!(false));
        assert_eq!(body["think"], serde_json::json!(false));
        assert_eq!(body["options"]["num_ctx"], serde_json::json!(16384));
    }

    #[test]
    fn openai_compat_never_gets_an_options_field() {
        for provider in ["openai", "lmstudio", "custom"] {
            let config = cfg(provider, "m");
            let (_, body) = build_body(
                config.provider_enum(),
                &config,
                &ChatRequest {
                    messages: vec![ChatMessage::user("hi")],
                    tools: vec![],
                    max_tokens: 64,
                    num_ctx: Some(16384),
                },
            )
            .unwrap();
            assert!(
                body.get("options").is_none(),
                "{provider} must not receive options"
            );
        }
    }

    #[test]
    fn recorded_ollama_text_reply_parses() {
        let value = serde_json::json!({
            "model": "gemma4:12b",
            "message": {"role": "assistant", "content": "BC-5 is blocked by BC-4."},
            "prompt_eval_count": 120,
            "eval_count": 18
        });
        match parse_reply(LlmProvider::Ollama, &value).unwrap().reply {
            ChatReply::Text(t) => assert!(t.contains("BC-4")),
            ChatReply::ToolCalls(_) => panic!("expected text"),
        }
    }

    #[test]
    fn recorded_ollama_empty_reply_is_an_error() {
        let value = serde_json::json!({
            "model": "x",
            "message": {"role": "assistant", "content": "   "}
        });
        assert!(matches!(
            parse_reply(LlmProvider::Ollama, &value),
            Err(LlmError::BadReply { .. })
        ));
    }

    #[test]
    fn recorded_openai_tool_call_parses() {
        let value = serde_json::json!({
            "choices": [{
                "message": {
                    "role": "assistant",
                    "content": null,
                    "tool_calls": [{
                        "id": "call_1",
                        "type": "function",
                        "function": {
                            "name": "media_catalog",
                            "arguments": "{\"operation\":\"stats\"}"
                        }
                    }]
                }
            }],
            "usage": {"prompt_tokens": 40, "completion_tokens": 12}
        });
        match parse_reply(LlmProvider::OpenAi, &value).unwrap().reply {
            ChatReply::ToolCalls(calls) => {
                assert_eq!(calls.len(), 1);
                assert_eq!(calls[0].name, "media_catalog");
            }
            ChatReply::Text(_) => panic!("expected tool calls"),
        }
    }

    #[test]
    fn recorded_anthropic_text_and_tool_use_parse() {
        let value = serde_json::json!({
            "content": [
                {"type": "text", "text": "Here is the count."},
                {"type": "tool_use", "id": "toolu_1", "name": "media_search",
                 "input": {"operation": "search", "query": "yotsuba"}}
            ],
            "usage": {"input_tokens": 30, "output_tokens": 9}
        });
        match parse_reply(LlmProvider::Anthropic, &value).unwrap().reply {
            ChatReply::ToolCalls(calls) => assert_eq!(calls[0].name, "media_search"),
            ChatReply::Text(_) => panic!("expected tool calls"),
        }
    }

    #[test]
    fn recorded_gemini_function_call_parses() {
        let value = serde_json::json!({
            "candidates": [{
                "content": {"parts": [
                    {"text": "One moment."},
                    {"functionCall": {
                        "name": "media_read",
                        "args": {"operation": "open", "key": "lib:1|film.mkv"}
                    }}
                ]}
            }]
        });
        match parse_reply(LlmProvider::Gemini, &value).unwrap().reply {
            ChatReply::ToolCalls(calls) => assert_eq!(calls[0].name, "media_read"),
            ChatReply::Text(_) => panic!("expected tool calls"),
        }
    }
}

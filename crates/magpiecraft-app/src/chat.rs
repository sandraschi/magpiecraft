//! In-app chat dock: Q&A grounded in this catalogue, with read-only tools.
//!
//! The worker pattern is mandatory: blocking provider calls run on a spawned
//! thread and the UI polls an mpsc channel every frame. The UI thread never
//! waits on the network. Cancel kills the curl child through the shared flag.
//!
//! The model can only call read-only tools, and they run directly through the
//! command bus. Anything else gets a refusal, never a confirm card: this chat
//! exposes zero writes, so there is nothing to confirm.

use magpiecraft_core::command::{dispatch, Command};
use magpiecraft_core::llm::{
    self, Cancel, ChatMessage, ChatReply, ChatRequest, LlmConfig, ToolSpec,
};
use magpiecraft_core::{Catalog, Paths};
use serde_json::{json, Value};
use std::sync::mpsc::{channel, Receiver};

/// Command ids the chat may run. Everything else is refused: no writes, no
/// launches, no config or VRAM changes, no network side effects.
const READ_ONLY: &[&str] = &[
    "status",
    "tools",
    "sources",
    "skill.read",
    "libraries",
    "browse",
    "search",
    "item",
    "pages",
    "progress.get",
    "continue",
    "stats",
    "duplicates",
    "hash",
    "manifest.verify",
    "series",
    "inspect",
    "settings",
    "review",
    "verify.status",
    "backup.status",
    "dupes.near",
    "subs",
    "presets",
    "plans",
    "journals",
];

/// One visible chat row.
#[derive(Debug, Clone)]
pub enum ChatLine {
    User(String),
    Assistant(String),
    Tool {
        name: String,
        args: Value,
        result: Value,
    },
    Error(String),
}

/// What the worker sends back when a turn finishes.
pub struct ChatTurn {
    pub lines: Vec<ChatLine>,
    pub prompt: String,
}

/// The dock state. Lives on the app; rendered as a bounded right panel.
pub struct ChatDock {
    pub open: bool,
    pub maximized: bool,
    pub input: String,
    pub lines: Vec<ChatLine>,
    pub busy: bool,
    rx: Option<Receiver<ChatTurn>>,
    cancel: Option<Cancel>,
    pub tools_off: bool,
    pub last_prompt: String,
    pub show_prompt: bool,
}

impl ChatDock {
    pub fn new() -> Self {
        ChatDock {
            open: false,
            maximized: false,
            input: String::new(),
            lines: Vec::new(),
            busy: false,
            rx: None,
            cancel: None,
            tools_off: false,
            last_prompt: String::new(),
            show_prompt: false,
        }
    }

    pub fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            cancel.cancel();
        }
    }

    /// Poll the worker. Call every frame; repaints while busy. Returns the
    /// lines that finished this frame so the app can transcript them.
    pub fn poll(&mut self, ctx: &egui::Context) -> Vec<ChatLine> {
        let mut fresh = Vec::new();
        let mut busy = false;
        if let Some(rx) = &self.rx {
            match rx.try_recv() {
                Ok(turn) => {
                    self.last_prompt = turn.prompt;
                    self.lines.extend(turn.lines.iter().cloned());
                    fresh.extend(turn.lines);
                    self.busy = false;
                    self.rx = None;
                    self.cancel = None;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => busy = true,
                Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                    self.lines.push(ChatLine::Error(
                        "chat worker stopped unexpectedly".to_string(),
                    ));
                    self.busy = false;
                    self.rx = None;
                    self.cancel = None;
                }
            }
        }
        if busy {
            ctx.request_repaint();
        }
        fresh
    }

    /// Send the input box. Builds the job and spawns the worker.
    pub fn send(
        &mut self,
        config: &LlmConfig,
        history_limit: usize,
        status: Value,
        selection: Value,
        paths: Paths,
    ) {
        let text = self.input.trim().to_string();
        if text.is_empty() || self.busy {
            return;
        }
        if let Err(e) = config.send_ready() {
            self.lines.push(ChatLine::Error(format!("{e}")));
            return;
        }
        self.input.clear();
        self.lines.push(ChatLine::User(text.clone()));
        let history: Vec<ChatMessage> = self
            .lines
            .iter()
            .rev()
            .take(history_limit * 2)
            .rev()
            .filter_map(|line| match line {
                ChatLine::User(t) => Some(ChatMessage::user(t.clone())),
                ChatLine::Assistant(t) => Some(ChatMessage {
                    role: llm::ChatRole::Assistant,
                    content: t.clone(),
                    tool_calls: Vec::new(),
                    tool_call_id: None,
                }),
                _ => None,
            })
            .collect();
        let specs = if self.tools_off || !config.tools_enabled {
            Vec::new()
        } else {
            tool_specs()
        };
        let job = ChatJob {
            config: config.clone(),
            history,
            user: text,
            specs,
            status,
            selection,
            paths,
        };
        let (tx, rx) = channel::<ChatTurn>();
        self.rx = Some(rx);
        self.busy = true;
        let cancel = Cancel::new();
        self.cancel = Some(cancel.clone());
        std::thread::Builder::new()
            .name("magpie-chat".to_string())
            .spawn(move || {
                let turn = run_turn(&job, &cancel);
                let _ = tx.send(turn);
            })
            .ok();
    }

    pub fn clear(&mut self) {
        self.cancel();
        self.lines.clear();
        self.last_prompt.clear();
        self.busy = false;
        self.rx = None;
    }
}

struct ChatJob {
    config: LlmConfig,
    history: Vec<ChatMessage>,
    user: String,
    specs: Vec<ToolSpec>,
    status: Value,
    selection: Value,
    paths: Paths,
}

/// System preprompt: header rules, skills, tool catalogue, live status,
/// selection, user extra. Capped parts truncate with a marker, never silently.
fn build_prompt(job: &ChatJob) -> String {
    use magpiecraft_core::skills::SKILLS;
    let version = magpiecraft_core::version();
    let mut out = format!(
        "You are the MagpieCraft {version} assistant. Today is {}. Answer from the context and tool results below. Say when you do not know. Never invent tool names, issue keys or numbers. Quote the tool result you relied on. You cannot change anything: renames, applies and undos live in the app, the CLI and MCP, never in this chat.\n",
        today()
    );
    out.push_str("\n## Skills\n");
    for skill in SKILLS {
        out.push_str(&format!(
            "### {}\n{}\n\n{}\n\n",
            skill.name,
            skill.description(),
            skill.body
        ));
    }
    if !job.specs.is_empty() {
        out.push_str("\n## Read-only tools (native calls; anything else does not exist)\n");
        for spec in &job.specs {
            out.push_str(&format!("- {}: {}\n", spec.name, spec.description));
        }
    }
    out.push_str("\n## Live catalogue status\n");
    push_capped(&mut out, &job.status.to_string(), 4000);
    out.push_str("\n## Current selection\n");
    push_capped(&mut out, &job.selection.to_string(), 4000);
    if !job.config.user_extra_prompt.trim().is_empty() {
        out.push_str("\n## User instructions\n");
        push_capped(&mut out, job.config.user_extra_prompt.trim(), 2000);
    }
    out
}

fn push_capped(out: &mut String, text: &str, cap: usize) {
    if text.len() <= cap {
        out.push_str(text);
        out.push('\n');
    } else {
        out.push_str(&text[..cap]);
        out.push_str("\n[truncated]\n");
    }
}

/// Gregorian date for the preprompt header, no extra dependency.
fn today() -> String {
    let days = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() / 86400)
        .unwrap_or(0) as i64;
    let (y, m, d) = civil_from_days(days + 719468);
    format!("{y:04}-{m:02}-{d:02}")
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// The read-only tools the model may call, with minimal schemas.
fn tool_specs() -> Vec<ToolSpec> {
    let key =
        json!({"type": "string", "description": "Item or folder key, for example lib:1|Manga"});
    let library = json!({"type": "string", "description": "Library name or numeric id"});
    let limit = json!({"type": "integer", "description": "Result cap"});
    let obj = |props: Value, required: &[&str]| {
        json!({"type": "object", "properties": props,
               "required": required, "additionalProperties": false})
    };
    vec![
        ToolSpec {
            name: "catalog_status".to_string(),
            description: "Version, home, counts, external tools".to_string(),
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "catalog_libraries".to_string(),
            description: "Configured libraries".to_string(),
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "catalog_stats".to_string(),
            description: "Counts, pages, bytes, per-library totals".to_string(),
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "catalog_browse".to_string(),
            description: "Browse one folder by key".to_string(),
            input_schema: obj(json!({"key": key}), &["key"]),
        },
        ToolSpec {
            name: "catalog_continue".to_string(),
            description: "Recently read items".to_string(),
            input_schema: obj(json!({"limit": limit}), &[]),
        },
        ToolSpec {
            name: "catalog_search".to_string(),
            description: "Whole-collection search; every word must match".to_string(),
            input_schema: obj(
                json!({"query": {"type": "string"}, "limit": limit}),
                &["query"],
            ),
        },
        ToolSpec {
            name: "catalog_item".to_string(),
            description: "Full record for one key".to_string(),
            input_schema: obj(json!({"key": key}), &["key"]),
        },
        ToolSpec {
            name: "skill_read".to_string(),
            description: "Read one bundled skill by name, or list them".to_string(),
            input_schema: obj(json!({"name": {"type": "string"}}), &[]),
        },
        ToolSpec {
            name: "health_verify_status".to_string(),
            description: "Which libraries are due for a re-check".to_string(),
            input_schema: obj(json!({}), &[]),
        },
        ToolSpec {
            name: "health_duplicates".to_string(),
            description: "Exact duplicate groups".to_string(),
            input_schema: obj(json!({"library": library, "limit": limit}), &[]),
        },
        ToolSpec {
            name: "health_inspect".to_string(),
            description: "ffprobe details for one file".to_string(),
            input_schema: obj(
                json!({"path": {"type": "string", "description": "Absolute media path"}}),
                &["path"],
            ),
        },
    ]
}

/// Map one model tool call to a read-only command. Anything else is refused.
fn map_tool_call(name: &str, args: &Value) -> Result<Command, String> {
    let str_arg = |key: &str| {
        args.get(key)
            .and_then(Value::as_str)
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
    };
    let req = |key: &str| str_arg(key).ok_or_else(|| format!("{name} needs {key}"));
    let limit = args
        .get("limit")
        .and_then(Value::as_u64)
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(12);
    match name {
        "catalog_status" => Ok(Command::Status),
        "catalog_libraries" => Ok(Command::Libraries),
        "catalog_stats" => Ok(Command::Stats),
        "catalog_browse" => Ok(Command::Browse {
            key: str_arg("key"),
        }),
        "catalog_continue" => Ok(Command::Continue { limit }),
        "catalog_search" => Ok(Command::Search {
            query: req("query")?,
            limit,
            mode: magpiecraft_core::SearchMode::Auto,
        }),
        "catalog_item" => Ok(Command::Item { key: req("key")? }),
        "skill_read" => Ok(Command::SkillRead {
            name: str_arg("name"),
        }),
        "health_verify_status" => Ok(Command::VerifyStatus { every_days: 30 }),
        "health_duplicates" => Ok(Command::Duplicates {
            library: str_arg("library"),
            limit,
        }),
        "health_inspect" => Ok(Command::Inspect { path: req("path")? }),
        _ => Err(format!(
            "{name} is not available in this chat; use the app, the CLI or MCP"
        )),
    }
}

/// Run at most 6 tool rounds, then the model must answer. Read-only tools run
/// directly; anything else is a refusal line, never a confirm card, because
/// this chat exposes no writes at all.
fn run_turn(job: &ChatJob, cancel: &Cancel) -> ChatTurn {
    let system = build_prompt(job);
    let mut messages = vec![ChatMessage::system(system.clone())];
    messages.extend(job.history.iter().cloned());
    messages.push(ChatMessage::user(job.user.clone()));
    let mut lines = Vec::new();
    let mut catalog = match Catalog::open(job.paths.clone()) {
        Ok(c) => c,
        Err(e) => {
            lines.push(ChatLine::Error(format!("cannot open the catalogue: {e}")));
            return ChatTurn {
                lines,
                prompt: system,
            };
        }
    };
    for _ in 0..6 {
        let req = ChatRequest {
            messages: messages.clone(),
            tools: job.specs.clone(),
            max_tokens: 1024,
            num_ctx: Some(16384),
        };
        let outcome = match llm::chat(&job.config, &req, cancel) {
            Ok(outcome) => outcome,
            Err(llm::LlmError::Cancelled) => {
                lines.push(ChatLine::Error("cancelled".to_string()));
                return ChatTurn {
                    lines,
                    prompt: system,
                };
            }
            Err(e) => {
                lines.push(ChatLine::Error(format!("{e}")));
                return ChatTurn {
                    lines,
                    prompt: system,
                };
            }
        };
        match outcome.reply {
            ChatReply::Text(text) => {
                lines.push(ChatLine::Assistant(text));
                return ChatTurn {
                    lines,
                    prompt: system,
                };
            }
            ChatReply::ToolCalls(calls) => {
                if job.specs.is_empty() {
                    lines.push(ChatLine::Assistant(
                        "Tools are off, so I answered from the catalogue snapshot above. Turn tools on to let me look things up.".to_string(),
                    ));
                    return ChatTurn {
                        lines,
                        prompt: system,
                    };
                }
                let mut assistant_parts = Vec::new();
                for call in calls {
                    match map_tool_call(&call.name, &call.arguments) {
                        Ok(cmd) => {
                            if !READ_ONLY.contains(&cmd.id()) {
                                lines.push(ChatLine::Error(format!(
                                    "{} is not available in this chat; use the app, the CLI or MCP",
                                    call.name
                                )));
                                continue;
                            }
                            let result = dispatch(&mut catalog, cmd);
                            assistant_parts.push(ChatLine::Tool {
                                name: call.name.clone(),
                                args: call.arguments.clone(),
                                result: result.clone(),
                            });
                            messages.push(ChatMessage {
                                role: llm::ChatRole::Tool,
                                content: truncate_result(&result),
                                tool_calls: Vec::new(),
                                tool_call_id: Some(call.id.clone()),
                            });
                        }
                        Err(refused) => {
                            lines.push(ChatLine::Error(refused));
                        }
                    }
                }
                lines.extend(assistant_parts);
            }
        }
    }
    lines.push(ChatLine::Error(
        "stopped after 6 tool rounds without an answer; ask something smaller".to_string(),
    ));
    ChatTurn {
        lines,
        prompt: system,
    }
}

pub(crate) fn truncate_result(value: &Value) -> String {
    let text = value.to_string();
    if text.len() <= 4000 {
        text
    } else {
        format!("{}... [truncated]", &text[..4000])
    }
}

/// Append-only transcript, outside the business DB and never holding keys.
pub fn append_transcript(home: &std::path::Path, lines: &[ChatLine]) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let days = (now / 86400) as i64;
    let (y, m, d) = civil_from_days(days + 719468);
    let dir = home.join("chat");
    let _ = std::fs::create_dir_all(&dir);
    let file = dir.join(format!("{y:04}-{m:02}-{d:02}.jsonl"));
    let mut body = String::new();
    for line in lines {
        let (role, text) = match line {
            ChatLine::User(t) => ("user", t.clone()),
            ChatLine::Assistant(t) => ("assistant", t.clone()),
            ChatLine::Tool { name, args, result } => (
                "tool",
                format!(
                    "{name} {} => {}",
                    truncate_result(args),
                    truncate_result(result)
                ),
            ),
            ChatLine::Error(t) => ("error", t.clone()),
        };
        body.push_str(&json!({"t": now, "role": role, "text": text}).to_string());
        body.push('\n');
    }
    if !body.is_empty() {
        let _ = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|mut f| {
                use std::io::Write as _;
                f.write_all(body.as_bytes())
            });
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn read_only_tools_map_and_writes_refuse() {
        let args = json!({"query": "yotsuba"});
        let cmd = map_tool_call("catalog_search", &args).unwrap();
        assert_eq!(cmd.id(), "search");
        assert!(READ_ONLY.contains(&cmd.id()));
        assert!(map_tool_call("media_curate", &json!({})).is_err());
        assert!(map_tool_call("open", &json!({})).is_err());
        assert!(map_tool_call("llm.select", &json!({})).is_err());
        assert!(map_tool_call("catalog_search", &json!({})).is_err());
    }

    #[test]
    fn every_mapped_command_is_allowlisted() {
        // If a mapping ever points at a write, the test names it.
        for (name, args) in [
            ("catalog_status", json!({})),
            ("catalog_libraries", json!({})),
            ("catalog_stats", json!({})),
            ("catalog_browse", json!({"key": "lib:1|Manga"})),
            ("catalog_continue", json!({})),
            ("catalog_search", json!({"query": "x"})),
            ("catalog_item", json!({"key": "lib:1|a.cbz"})),
            ("skill_read", json!({})),
            ("health_verify_status", json!({})),
            ("health_duplicates", json!({})),
            ("health_inspect", json!({"path": "D:/x.mkv"})),
        ] {
            let cmd = map_tool_call(name, &args).unwrap();
            assert!(
                READ_ONLY.contains(&cmd.id()),
                "{name} maps to {}, which is not read-only",
                cmd.id()
            );
        }
    }

    #[test]
    fn preprompt_dates_and_truncates() {
        let (y, m, d) = civil_from_days(719468 + 20000);
        assert!(y > 2020 && (1..=12).contains(&m) && (1..=31).contains(&d));
        assert!(today().len() == 10);
        let mut out = String::new();
        push_capped(&mut out, "abcdef", 4);
        assert!(out.contains("[truncated]"));
    }
}

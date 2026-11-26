//! MCP stdio transport.
//!
//! Line-delimited JSON-RPC 2.0, MCP `2024-11-05`. stdout carries protocol messages
//! only; every log line goes to stderr. A notification (no `id`) never gets a
//! reply, which is the one framing rule clients fail hardest on.

use crate::tool;
use magpiecraft_core::{Catalog, Paths};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

pub fn serve(paths: Paths) -> Result<(), Box<dyn std::error::Error>> {
    let mut catalog = Catalog::open(paths)?;
    eprintln!(
        "[magpiecraft-mcp] stdio server ready ({}), {} libraries",
        env!("CARGO_PKG_VERSION"),
        catalog.libraries().map(|l| l.len()).unwrap_or(0)
    );
    let stdin = std::io::stdin();
    let mut out = std::io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(line) {
            Ok(message) => handle(&mut catalog, &message),
            Err(e) => Some(json!({
                "jsonrpc": "2.0",
                "id": Value::Null,
                "error": { "code": -32700, "message": format!("Parse error: {e}") },
            })),
        };
        if let Some(reply) = reply {
            writeln!(out, "{reply}")?;
            out.flush()?;
        }
    }
    Ok(())
}

/// The skill folder from a `skill://<name>/SKILL.md` URI. Anything else is None,
/// which becomes a -32002 instead of an empty success.
fn skill_name_from_uri(uri: &str) -> Option<&str> {
    let rest = uri.strip_prefix("skill://")?;
    rest.strip_suffix("/SKILL.md")
}

pub fn handle(catalog: &mut Catalog, message: &Value) -> Option<Value> {
    let id = message.get("id")?.clone();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let reply = |result: Value| json!({ "jsonrpc": "2.0", "id": id, "result": result });
    Some(match method {
        "initialize" => reply(json!({
            "protocolVersion": "2024-11-05",
            "capabilities": { "tools": {}, "resources": { "listChanged": false } },
            "serverInfo": {
                "name": "magpiecraft-mcp",
                "version": env!("CARGO_PKG_VERSION"),
            },
            "instructions": "MagpieCraft inventories and reads a media collection the user already owns. \
                             It never downloads, never circumvents DRM and never writes into a library. \
                             Start with media_catalog {operation:\"status\"}."
        })),
        "ping" => reply(json!({})),
        "tools/list" => reply(json!({ "tools": tool::tools() })),
        "tools/call" => reply(tool::call_tool(
            catalog,
            message.get("params").unwrap_or(&Value::Null),
        )),
        "resources/list" => {
            let resources: Vec<Value> = magpiecraft_core::skills::SKILLS
                .iter()
                .map(|s| {
                    json!({
                        "uri": magpiecraft_core::skills::skill_uri(s.name),
                        "name": s.name,
                        "description": s.description(),
                        "mimeType": "text/markdown",
                    })
                })
                .collect();
            reply(json!({ "resources": resources }))
        }
        "resources/read" => {
            let uri = message
                .get("params")
                .and_then(|p| p.get("uri"))
                .and_then(Value::as_str)
                .unwrap_or("");
            match skill_name_from_uri(uri).and_then(magpiecraft_core::skills::find) {
                Some(skill) => reply(json!({
                    "contents": [{
                        "uri": uri,
                        "mimeType": "text/markdown",
                        "text": skill.body,
                    }]
                })),
                None => json!({
                    "jsonrpc": "2.0",
                    "id": id,
                    "error": { "code": -32002, "message": "Resource not found" },
                }),
            }
        }
        "prompts/list" => reply(json!({ "prompts": [] })),
        "logging/setLevel" => reply(json!({})),
        _ => crate::method_not_found(id),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::tool;

    fn catalog() -> Catalog {
        let home = std::env::temp_dir().join(format!("magpie-mcp-loop-{}", std::process::id()));
        Catalog::open_in_memory(Paths::at(home)).unwrap()
    }

    #[test]
    fn notifications_are_not_answered() {
        let mut c = catalog();
        assert!(handle(
            &mut c,
            &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .is_none());
    }

    #[test]
    fn handshake_then_tools_list_then_a_call() {
        let mut c = catalog();
        let init = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":1,"method":"initialize"}),
        )
        .unwrap();
        assert_eq!(init["result"]["protocolVersion"], json!("2024-11-05"));
        assert_eq!(
            init["result"]["serverInfo"]["name"],
            json!("magpiecraft-mcp")
        );
        let list = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
        )
        .unwrap();
        let tools = list["result"]["tools"].as_array().unwrap();
        assert_eq!(tools.len(), 13);
        let names: Vec<&str> = tools.iter().filter_map(|t| t["name"].as_str()).collect();
        assert!(names.contains(&"media_curate"));
        assert!(names.contains(&"media_transform"));
        assert!(names.contains(&"magpie_skill_read"));
        assert!(names.contains(&"magpie_llm_status"));
        assert!(names.contains(&"magpie_llm_test"));
        let call = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{
                "name":"media_catalog","arguments":{"operation":"status"}
            }}),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], json!(false));
    }

    #[test]
    fn resources_serve_every_bundled_skill_byte_for_byte() {
        let mut c = catalog();
        let list = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":4,"method":"resources/list"}),
        )
        .unwrap();
        let resources = list["result"]["resources"].as_array().unwrap();
        assert_eq!(resources.len(), magpiecraft_core::skills::SKILLS.len());
        for r in resources {
            let uri = r["uri"].as_str().unwrap();
            let read = handle(
                &mut c,
                &json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":uri}}),
            )
            .unwrap();
            let text = read["result"]["contents"][0]["text"].as_str().unwrap();
            let name = uri
                .strip_prefix("skill://")
                .unwrap()
                .strip_suffix("/SKILL.md")
                .unwrap();
            let skill = magpiecraft_core::skills::find(name).unwrap();
            assert_eq!(text, skill.body);
        }
        let missing = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":"skill://nope/SKILL.md"}}),
        )
        .unwrap();
        assert_eq!(missing["error"]["code"], json!(-32002));
        let call = handle(
            &mut c,
            &json!({"jsonrpc":"2.0","id":7,"method":"tools/call","params":{
                "name":"magpie_skill_read","arguments":{"name":"magpiecraft-video"}
            }}),
        )
        .unwrap();
        assert_eq!(call["result"]["isError"], json!(false));
    }

    #[test]
    fn unknown_method_is_an_error_not_a_crash() {
        let mut c = catalog();
        let out = handle(&mut c, &json!({"jsonrpc":"2.0","id":9,"method":"nope"})).unwrap();
        assert_eq!(out["error"]["code"], json!(-32601));
        let _ = tool::tools();
    }
}

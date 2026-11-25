//! Human-readable output for the CLI. `--json` is always available and is what
//! scripts and agents should use; this is for reading at a terminal.

use serde_json::Value;

pub fn bytes(n: i64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = n as f64;
    let mut unit = 0usize;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{n} B")
    } else {
        format!("{value:.1} {}", UNITS.get(unit).copied().unwrap_or("B"))
    }
}

fn s(v: &Value, key: &str) -> String {
    v.get(key).and_then(Value::as_str).unwrap_or("").to_string()
}

fn n(v: &Value, key: &str) -> i64 {
    v.get(key).and_then(Value::as_i64).unwrap_or(0)
}

/// A list result is wrapped by `dispatch` as `{"ok": true, "value": [...]}`, so the
/// printer has to look inside `value` before giving up.
fn list_of(value: &Value) -> Option<&Vec<Value>> {
    value
        .get("value")
        .and_then(Value::as_array)
        .or_else(|| value.as_array())
}

pub fn print(command: &str, value: &Value) {
    if value.get("ok").and_then(Value::as_bool) == Some(false) {
        println!("error ({}): {}", s(value, "error_type"), s(value, "error"));
        return;
    }
    match command {
        "status" => {
            println!("MagpieCraft {} ({})", s(value, "version"), s(value, "home"));
            println!(
                "libraries {}  items {}  missing {}  pages {}  {}",
                n(value, "libraries"),
                n(value, "items"),
                n(value, "missing"),
                n(value, "pages"),
                bytes(n(value, "bytes"))
            );
            if n(value, "mature_visible") == 0
                && value.get("mature_visible") == Some(&Value::Bool(false))
            {
                println!("mature libraries: hidden (Settings to show)");
            }
            let tools = value.get("tools").cloned().unwrap_or(Value::Null);
            let mut missing = Vec::new();
            for name in ["ffprobe", "seven_zip", "pdfcraft"] {
                if tools
                    .get(name)
                    .and_then(|t| t.get("found"))
                    .and_then(Value::as_bool)
                    != Some(true)
                {
                    missing.push(name);
                }
            }
            if missing.is_empty() {
                println!("external tools: ffprobe, 7-Zip, PdfCraft all found");
            } else {
                println!("external tools missing: {}", missing.join(", "));
            }
        }
        "libraries" => match list_of(value) {
            Some(list) if !list.is_empty() => {
                for lib in list {
                    println!(
                        "{:>2}  {:<22} {:<10} {}",
                        n(lib, "id"),
                        s(lib, "name"),
                        s(lib, "kind"),
                        s(lib, "root")
                    );
                }
            }
            _ => {
                println!("no libraries yet: magpiecraft-mcp library-add <name> <root> --kind manga")
            }
        },
        "library.add" => println!(
            "added library {} (id {}) at {}",
            s(value, "name"),
            n(value, "id"),
            s(value, "root")
        ),
        "library.remove" => println!(
            "removed library {} ({} index rows dropped, no files touched)",
            s(value, "removed"),
            n(value, "items_dropped_from_index")
        ),
        "scan" => {
            let mut any = false;
            if let Some(list) = value.get("libraries").and_then(Value::as_array) {
                for lib in list {
                    any = true;
                    if lib.get("skipped").and_then(Value::as_bool) == Some(true) {
                        println!("{}: skipped ({})", s(lib, "library"), s(lib, "note"));
                        continue;
                    }
                    println!(
                        "{}: +{} new, {} changed, {} unchanged, {} missing, {} dirs, {} ms",
                        s(lib, "library"),
                        n(lib, "added"),
                        n(lib, "updated"),
                        n(lib, "unchanged"),
                        n(lib, "missing"),
                        n(lib, "dirs"),
                        n(lib, "ms")
                    );
                }
            }
            if !any {
                println!("nothing to scan: add a library first");
            } else {
                println!(
                    "total: +{} new, {} changed, {} missing in {} ms",
                    n(value, "added"),
                    n(value, "updated"),
                    n(value, "missing"),
                    n(value, "ms")
                );
            }
        }
        "browse" => {
            let crumbs = value
                .get("crumbs")
                .and_then(Value::as_array)
                .map(|c| {
                    c.iter()
                        .map(|x| s(x, "name"))
                        .collect::<Vec<_>>()
                        .join(" / ")
                })
                .unwrap_or_default();
            println!("{crumbs}");
            if let Some(folders) = value.get("folders").and_then(Value::as_array) {
                for f in folders {
                    println!("  [dir]  {:<40} {} items", s(f, "name"), n(f, "items"));
                }
            }
            if let Some(items) = value.get("items").and_then(Value::as_array) {
                for item in items {
                    let pages = match item.get("page_count").and_then(Value::as_i64) {
                        Some(p) => format!("{p} p"),
                        None => "-".to_string(),
                    };
                    println!(
                        "  {:<9} {:<44} {:<6} {}",
                        s(item, "kind"),
                        s(item, "title"),
                        pages,
                        s(item, "key")
                    );
                }
            }
            if n(value, "total_folders") == 0 && n(value, "total_items") == 0 {
                println!("  (nothing readable here)");
            }
        }
        "search" => {
            let hits = value
                .get("hits")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            println!(
                "{} result{}{} via {}",
                n(value, "count"),
                if n(value, "count") == 1 { "" } else { "s" },
                if value.get("truncated").and_then(Value::as_bool) == Some(true) {
                    "+"
                } else {
                    ""
                },
                s(value, "mode")
            );
            for hit in hits.iter().take(40) {
                let where_ = s(hit, "where_");
                println!(
                    "  {:<44} {} / {}",
                    s(hit, "title"),
                    s(hit, "library"),
                    if where_.is_empty() { "." } else { &where_ }
                );
            }
        }
        "pages" => {
            println!(
                "{}  ({} pages, {})",
                s(value, "title"),
                n(value, "count"),
                s(value, "container")
            );
            if value.get("readable").and_then(Value::as_bool) == Some(false) {
                println!("  not readable: {}", s(value, "reason"));
            }
            if value.get("rtl").and_then(Value::as_bool) == Some(true) {
                println!("  right to left");
            }
            if let Some(preview) = value.get("preview").and_then(Value::as_array) {
                let names: Vec<String> = preview
                    .iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect();
                if !names.is_empty() {
                    println!("  first pages: {}", names.join(", "));
                }
            }
        }
        "page_export" => println!(
            "wrote page {} ({}) to {} ({} bytes)",
            n(value, "page"),
            s(value, "page_name"),
            s(value, "out"),
            n(value, "bytes")
        ),
        "open" => {
            println!(
                "opened {} in {}{}",
                s(value, "title"),
                s(value, "player"),
                match n(value, "start_secs") {
                    0 => String::new(),
                    secs => format!(" at {}s", secs),
                }
            );
            println!("  {}", s(value, "path"));
        }
        "review" => {
            println!(
                "{} item{} in the review queue ({} in total)",
                n(value, "count"),
                if n(value, "count") == 1 { "" } else { "s" },
                n(value, "total_in_queue")
            );
            if let Some(entries) = value.get("entries").and_then(Value::as_array) {
                for e in entries.iter().take(30) {
                    println!(
                        "  {:.2}  {:<44} {}",
                        e.get("confidence").and_then(Value::as_f64).unwrap_or(0.0),
                        s(e, "rel"),
                        s(e, "key")
                    );
                }
            }
        }
        "plan.rename" | "plan.quarantine" | "plan.cleanup" => {
            println!(
                "{} operation{} planned ({} skipped)",
                n(value, "operations"),
                if n(value, "operations") == 1 { "" } else { "s" },
                value
                    .get("plans")
                    .and_then(Value::as_array)
                    .map(|p| p.iter().map(|x| n(x, "skipped")).sum::<i64>())
                    .unwrap_or(0)
            );
            if let Some(plans) = value.get("plans").and_then(Value::as_array) {
                for plan in plans {
                    println!("  plan {} ({})", s(plan, "id"), s(plan, "kind"));
                    for line in s(plan, "diff").lines().take(20) {
                        println!("    {line}");
                    }
                }
            }
            println!("  {}", s(value, "note"));
        }
        "backup.status" => {
            if value.get("unconfigured").and_then(Value::as_bool) == Some(true) {
                println!("no backup roots configured");
            } else {
                println!(
                    "{} complete, {} partial, {} not backed up",
                    n(value, "complete"),
                    n(value, "partial"),
                    n(value, "not_backed_up")
                );
            }
            if let Some(entries) = value.get("entries").and_then(Value::as_array) {
                for e in entries.iter().take(30) {
                    println!(
                        "  {:<16} {:<22} {} items",
                        s(e, "status"),
                        s(e, "library"),
                        n(e, "items")
                    );
                }
            }
            println!("  {}", s(value, "note"));
        }
        "backup.roots" => println!(
            "{} backup root(s): {}",
            n(value, "count"),
            value
                .get("backup_roots")
                .and_then(Value::as_array)
                .map(|r| r
                    .iter()
                    .filter_map(|x| x.as_str())
                    .collect::<Vec<_>>()
                    .join(", "))
                .unwrap_or_default()
        ),
        "verify.status" => {
            println!(
                "{} of {} libraries are due a re-check (every {} days)",
                n(value, "due"),
                n(value, "libraries"),
                n(value, "every_days")
            );
            if let Some(entries) = value.get("entries").and_then(Value::as_array) {
                for e in entries {
                    let last = e
                        .get("last_checked_at")
                        .and_then(Value::as_str)
                        .unwrap_or("never");
                    println!(
                        "  {:<3} {:<24} {:<22} last ok {} / missing {} / changed {}",
                        if e.get("due").and_then(Value::as_bool) == Some(true) {
                            "DUE"
                        } else {
                            "-"
                        },
                        s(e, "library"),
                        last,
                        e.get("last_ok").and_then(Value::as_i64).unwrap_or(0),
                        e.get("last_missing").and_then(Value::as_i64).unwrap_or(0),
                        e.get("last_changed").and_then(Value::as_i64).unwrap_or(0)
                    );
                }
            }
        }
        "verify.run" => {
            println!(
                "checked {} file(s) in {} librar{} ({} problem{})",
                n(value, "files_checked"),
                n(value, "libraries_checked"),
                if n(value, "libraries_checked") == 1 {
                    "y"
                } else {
                    "ies"
                },
                n(value, "problems"),
                if n(value, "problems") == 1 { "" } else { "s" }
            );
            if let Some(results) = value.get("results").and_then(Value::as_array) {
                for r in results {
                    println!(
                        "  {:<24} {} checked {} ok {}{}",
                        s(r, "library"),
                        if r.get("baseline").and_then(Value::as_bool) == Some(true) {
                            "baseline,"
                        } else {
                            "compared,"
                        },
                        n(r, "checked"),
                        n(r, "ok"),
                        if n(r, "changed") + n(r, "missing") > 0 {
                            format!(", {} changed, {} missing", n(r, "changed"), n(r, "missing"))
                        } else {
                            String::new()
                        }
                    );
                }
            }
            println!("  {}", s(value, "note"));
        }
        "plans" => {
            println!("{} saved plan(s)", n(value, "count"));
            if let Some(plans) = value.get("plans").and_then(Value::as_array) {
                for plan in plans.iter().take(20) {
                    println!(
                        "  {:<24} {:<10} {:<20} {} ops",
                        s(plan, "id"),
                        s(plan, "kind"),
                        s(plan, "library"),
                        n(plan, "operations")
                    );
                }
            }
        }
        "apply" => {
            println!(
                "applied {} operation(s), {} failed (journal {})",
                n(value, "applied"),
                n(value, "failed"),
                s(value, "journal")
            );
            if let Some(errors) = value.get("errors").and_then(Value::as_array) {
                for e in errors {
                    if let Some(text) = e.as_str() {
                        println!("  error: {text}");
                    }
                }
            }
            println!("  reverse with: {}", s(value, "undo"));
        }
        "journals" => {
            println!("{} journal entry/entries", n(value, "count"));
            if let Some(list) = value.get("journals").and_then(Value::as_array) {
                for j in list.iter().take(20) {
                    println!(
                        "  {:<28} {:<10} {:<20} {} applied{}",
                        s(j, "id"),
                        s(j, "kind"),
                        s(j, "library"),
                        n(j, "applied"),
                        if j.get("undone").and_then(Value::as_bool) == Some(true) {
                            ", undone"
                        } else {
                            ""
                        }
                    );
                }
            }
        }
        "undo" => {
            println!(
                "restored {} operation(s) (journal {})",
                n(value, "restored"),
                s(value, "journal")
            );
            if let Some(problems) = value.get("problems").and_then(Value::as_array) {
                for p in problems {
                    if let Some(text) = p.as_str() {
                        println!("  problem: {text}");
                    }
                }
            }
        }
        "stats" => {
            println!(
                "{} libraries, {} items, {} pages, {}",
                n(value, "libraries"),
                n(value, "items"),
                n(value, "pages"),
                bytes(n(value, "bytes"))
            );
            if let Some(list) = value.get("per_library").and_then(Value::as_array) {
                for lib in list {
                    println!(
                        "  {:<22} {:>7} items  {:>7} pages  {:>10}",
                        s(lib, "name"),
                        n(lib, "items"),
                        n(lib, "pages"),
                        bytes(n(lib, "bytes"))
                    );
                }
            }
            if let Some(kinds) = value.get("per_kind").and_then(Value::as_array) {
                if !kinds.is_empty() {
                    println!("  by kind:");
                }
                for k in kinds {
                    println!(
                        "    {:<10} {:>7} items  {:>10}",
                        s(k, "kind"),
                        n(k, "items"),
                        bytes(n(k, "bytes"))
                    );
                }
            }
            if let Some(drives) = value.get("drives").and_then(Value::as_array) {
                for d in drives {
                    println!(
                        "  drive {:<8} {:<8} {} items",
                        s(d, "volume"),
                        if d.get("online").and_then(Value::as_bool) == Some(true) {
                            "online"
                        } else {
                            "offline"
                        },
                        n(d, "items")
                    );
                }
            }
        }
        "duplicates" => {
            println!(
                "{} duplicate groups ({} hashed){}",
                n(value, "groups_total"),
                n(value, "files_hashed"),
                if value.get("truncated").and_then(Value::as_bool) == Some(true) {
                    ", truncated"
                } else {
                    ""
                }
            );
            if let Some(groups) = value.get("groups").and_then(Value::as_array) {
                for g in groups {
                    let paths: Vec<String> = g
                        .get("paths")
                        .and_then(Value::as_array)
                        .map(|p| {
                            p.iter()
                                .filter_map(|x| x.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    println!("  {} x {}", paths.len(), bytes(n(g, "size")));
                    for p in paths {
                        println!("    {p}");
                    }
                }
            }
        }
        "hash" => println!("{}  {}", s(value, "hash"), s(value, "key")),
        "manifest.write" => println!(
            "manifest {} ({} files, {})",
            s(value, "manifest"),
            n(value, "files"),
            bytes(n(value, "bytes"))
        ),
        "manifest.verify" => {
            println!(
                "{}: {} checked, {} ok, {} missing, {} changed",
                s(value, "manifest"),
                n(value, "checked"),
                n(value, "ok"),
                value
                    .get("missing")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0) as i64,
                value
                    .get("changed")
                    .and_then(Value::as_array)
                    .map(|a| a.len())
                    .unwrap_or(0) as i64
            );
            if value.get("clean").and_then(Value::as_bool) == Some(true) {
                println!("clean: every listed file matches its checksum");
            }
        }
        "series" => match list_of(value) {
            Some(list) if !list.is_empty() => {
                for ser in list {
                    println!("{:<44} {}", s(ser, "title"), s(ser, "rel"));
                }
            }
            _ => println!("no series yet: scan a library first"),
        },
        "continue" => {
            let entries = value
                .get("entries")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            if entries.is_empty() {
                println!("nothing in progress");
            }
            for e in &entries {
                println!(
                    "  {:<9} {:<14} {:<40} {}",
                    s(e, "kind"),
                    s(e, "label"),
                    s(e, "title"),
                    s(e, "key")
                );
            }
        }
        "settings" => println!(
            "theme {}  zoom {:.2}  mature visible {}  page folder >= {} images",
            s(value, "theme"),
            value.get("zoom").and_then(Value::as_f64).unwrap_or(1.0),
            value
                .get("mature_visible")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            n(value, "imgdir_min_pages")
        ),
        _ => match serde_json::to_string(value) {
            Ok(text) => println!("{text}"),
            Err(e) => println!("(unprintable output: {e})"),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn byte_sizes_are_readable() {
        assert_eq!(bytes(0), "0 B");
        assert_eq!(bytes(1023), "1023 B");
        assert_eq!(bytes(1024), "1.0 KiB");
        assert_eq!(bytes(4_300_000_000), "4.0 GiB");
    }

    #[test]
    fn a_wrapped_list_is_still_printed() {
        // dispatch wraps a list under `value`; the printer must look inside.
        let wrapped = json!({"ok": true, "command": "libraries", "value": [
            {"id": 1, "name": "Comics", "kind": "comics", "root": "L:/x"}
        ]});
        assert_eq!(list_of(&wrapped).map(|l| l.len()), Some(1));
        print("libraries", &wrapped);
        let bare = json!([{"id": 1, "name": "Comics", "kind": "comics", "root": "L:/x"}]);
        assert_eq!(list_of(&bare).map(|l| l.len()), Some(1));
    }

    #[test]
    fn printing_does_not_panic_on_odd_shapes() {
        print("status", &json!({"ok": true}));
        print(
            "browse",
            &json!({"ok": true, "crumbs": [{"name":"Libraries"}]}),
        );
        print(
            "search",
            &json!({"ok": true, "hits": [{"title":"x","library":"l","where_":""}]}),
        );
        print("nonsense", &json!({"ok": true, "a": 1}));
        print(
            "status",
            &json!({"ok": false, "error": "boom", "error_type": "invalid"}),
        );
    }
}

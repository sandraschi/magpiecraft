//! Bundled skills: one source for agents, the chat model and the human.
//!
//! Each skill is a folder under `skills/` with a `SKILL.md` in Agent Skills
//! format (YAML frontmatter with `name` and `description`, then markdown).
//! The bodies compile into the binary with `include_str!`, so there is no
//! file to install next to the exe. The MCP server answers `resources/list`
//! and `resources/read` from this bundle, the CLI exports it, and the Help
//! Center renders it.
//!
//! A skill names only tools the app really has. The test below fails the
//! build when a skill mentions a backticked tool that does not exist.

/// One bundled skill.
#[derive(Debug, Clone, Copy)]
pub struct SkillSrc {
    /// Folder name, also the frontmatter name.
    pub name: &'static str,
    /// Full SKILL.md body.
    pub body: &'static str,
}

impl SkillSrc {
    /// One sentence from frontmatter, at most 200 characters. Parsed at
    /// runtime; the static only holds the body.
    pub fn description(self) -> &'static str {
        frontmatter_field(self.body, "description")
    }
}

macro_rules! skill {
    ($name:literal) => {
        SkillSrc {
            name: $name,
            body: include_str!(concat!("../../../skills/", $name, "/SKILL.md")),
        }
    };
}

/// Every bundled skill. Adding a folder under `skills/` without listing it
/// here fails the bundle test.
pub static SKILLS: &[SkillSrc] = &[
    skill!("magpiecraft-workflow"),
    skill!("magpiecraft-comics"),
    skill!("magpiecraft-manga"),
    skill!("magpiecraft-magazines"),
    skill!("magpiecraft-video"),
    skill!("magpiecraft-audio"),
    skill!("magpiecraft-ebooks"),
    skill!("magpiecraft-photos"),
    skill!("magpiecraft-curate"),
    skill!("magpiecraft-verify"),
];

/// MCP tool names the app really serves. Skills may only name these.
pub const KNOWN_TOOLS: &[&str] = &[
    "media_catalog",
    "media_search",
    "media_read",
    "media_transform",
    "media_health",
    "media_curate",
    "media_onboard",
    "magpie_skill_read",
    "magpie_llm_status",
    "magpie_llm_list_models",
    "magpie_llm_select",
    "magpie_llm_set_key",
    "magpie_llm_evict",
    "magpie_llm_test",
];

/// Read one frontmatter field from a SKILL.md body. Returns "" when absent;
/// the tests treat that as a failure, not a default.
fn frontmatter_field<'a>(body: &'a str, field: &str) -> &'a str {
    let mut lines = body.lines();
    if lines.next() != Some("---") {
        return "";
    }
    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some(rest) = trimmed.strip_prefix(field) {
            if let Some(value) = rest.strip_prefix(':') {
                return value.trim();
            }
        }
    }
    ""
}

/// The skill URI served over MCP and used in docs.
pub fn skill_uri(name: &str) -> String {
    format!("skill://{name}/SKILL.md")
}

/// Look a skill up by folder name.
pub fn find(name: &str) -> Option<&'static SkillSrc> {
    SKILLS.iter().find(|s| s.name == name.trim())
}

/// Engine read for `Command::SkillRead`: the catalogue of skills without a
/// name, one full body with a name. Unknown names are an invalid error, never
/// an empty success.
pub fn read(name: Option<&str>) -> crate::MagpieResult<serde_json::Value> {
    match name.map(str::trim).filter(|n| !n.is_empty()) {
        None => {
            let list: Vec<serde_json::Value> = SKILLS
                .iter()
                .map(|s| {
                    serde_json::json!({
                        "name": s.name,
                        "description": s.description(),
                        "uri": skill_uri(s.name),
                    })
                })
                .collect();
            Ok(serde_json::json!({ "skills": list }))
        }
        Some(want) => match find(want) {
            Some(skill) => Ok(serde_json::json!({
                "name": skill.name,
                "description": skill.description(),
                "uri": skill_uri(skill.name),
                "body": skill.body,
            })),
            None => Err(crate::MagpieError::invalid(format!(
                "unknown skill: {want}"
            ))),
        },
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;

    fn skill_dirs() -> Vec<String> {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../skills");
        let mut names: Vec<String> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn every_skill_folder_is_bundled() {
        let bundled: Vec<&str> = SKILLS.iter().map(|s| s.name).collect();
        for dir in skill_dirs() {
            assert!(
                bundled.contains(&dir.as_str()),
                "skills/{dir} exists but is not in SKILLS"
            );
        }
        assert_eq!(
            bundled.len(),
            skill_dirs().len(),
            "bundle lists a missing folder"
        );
    }

    #[test]
    fn frontmatter_names_match_and_descriptions_fit() {
        for skill in SKILLS {
            let name = frontmatter_field(skill.body, "name");
            assert_eq!(
                name, skill.name,
                "frontmatter name mismatch in {}",
                skill.name
            );
            assert!(
                !skill.description().is_empty(),
                "{} has no description",
                skill.name
            );
            assert!(
                skill.description().chars().count() <= 200,
                "{} description is over 200 characters",
                skill.name
            );
        }
    }

    #[test]
    fn skills_reference_only_real_tools() {
        for skill in SKILLS {
            let mut rest = skill.body;
            while let Some(start) = rest.find('`') {
                let after = &rest[start + 1..];
                let Some(end) = after.find('`') else {
                    break;
                };
                let token = &after[..end];
                if token.starts_with("media_") || token.starts_with("magpie_") {
                    assert!(
                        KNOWN_TOOLS.contains(&token),
                        "{} names unknown tool `{token}`",
                        skill.name
                    );
                }
                rest = &after[end + 1..];
            }
        }
    }

    #[test]
    fn skills_stay_small_enough_for_a_local_model() {
        let mut total = 0;
        for skill in SKILLS {
            let bytes = skill.body.len();
            assert!(
                bytes <= 12 * 1024,
                "{} is {bytes} bytes, over the 12 KB cap",
                skill.name
            );
            total += bytes;
        }
        assert!(
            total <= 40 * 1024,
            "all skills are {total} bytes, over the 40 KB cap"
        );
    }
}

use anyhow::{Context, Result};
use rayon::prelude::*;
use serde::Serialize;
use std::path::{Path, PathBuf};

/// A globally (user-scope) installed plugin and what it ships.
#[derive(Debug, Clone, Serialize)]
pub struct Plugin {
    pub id: String, // "name@marketplace"
    pub name: String,
    pub marketplace: String,
    pub version: String,
    // ponytail: kept though only tests read it today — v2 skill extraction copies from here.
    #[serde(skip)]
    #[allow(dead_code)]
    pub install_path: PathBuf,
    pub provides: Vec<String>,
    pub skills: Vec<Skill>,
    // Always-on tokens from agents/commands (Claude Code counts their descriptions too).
    #[serde(skip)]
    pub extra_tokens: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub owner: String, // "plugin" | "loose"
}

/// Per-entry listing overhead Claude Code adds around each skill/agent/command (measured against
/// `claude plugin details`: ~25-40 tok per skill, ~20 per agent/command).
const SKILL_OVERHEAD: u32 = 30;
const EXTRA_OVERHEAD: u32 = 20;

/// Rough chars→tokens heuristic (~4 chars/token). For relative ranking, not billing.
pub fn est_tokens(text: &str) -> u32 {
    (text.chars().count() / 4) as u32
}

impl Plugin {
    /// Estimated always-on token cost = Σ over skills of est_tokens(name + description),
    /// plus agent/command descriptions. Bodies are lazy and not counted.
    pub fn est_tokens(&self) -> u32 {
        let skills: u32 = self
            .skills
            .iter()
            .map(|s| SKILL_OVERHEAD + est_tokens(&format!("{}\n{}", s.name, s.description)))
            .sum();
        skills + self.extra_tokens
    }
}

/// Load every user-scope-installed plugin from installed_plugins.json, with skills + provides.
pub fn load(home: &Path) -> Result<Vec<Plugin>> {
    let path = home.join(".claude/plugins/installed_plugins.json");
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(_) => return Ok(Vec::new()), // no plugins installed
    };
    let root: serde_json::Value =
        serde_json::from_str(&text).with_context(|| format!("parsing {}", path.display()))?;

    let map = match root.get("plugins").and_then(|p| p.as_object()) {
        Some(m) => m,
        None => return Ok(Vec::new()),
    };

    // Collect (id, user-scope record) pairs first, then parse skills in parallel.
    let entries: Vec<(String, PathBuf, String)> = map
        .iter()
        .filter_map(|(id, recs)| {
            let arr = recs.as_array()?;
            let user = arr
                .iter()
                .find(|r| r.get("scope").and_then(|s| s.as_str()) == Some("user"))?;
            let install = user.get("installPath").and_then(|p| p.as_str())?;
            let version = user
                .get("version")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            Some((id.clone(), PathBuf::from(install), version))
        })
        .collect();

    let mut plugins: Vec<Plugin> = entries
        .par_iter()
        .map(|(id, install_path, version)| {
            let (name, marketplace) = split_id(id);
            let skills = scan_skills(install_path, "plugin");
            let provides = detect_provides(install_path, &skills);
            Plugin {
                id: id.clone(),
                name,
                marketplace,
                version: version.clone(),
                install_path: install_path.clone(),
                provides,
                skills,
                extra_tokens: scan_extra_tokens(install_path),
            }
        })
        .collect();

    plugins.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(plugins)
}

/// Loose (project/user) skills living in `.claude/skills/`, governable via skillOverrides.
pub fn loose_skills(project: &Path, home: &Path) -> Vec<Skill> {
    let mut out = scan_skills(&project.join(".claude"), "loose");
    out.extend(scan_skills(&home.join(".claude"), "loose"));
    out
}

/// Split "name@marketplace" on the last `@`. No `@` → marketplace = "".
fn split_id(id: &str) -> (String, String) {
    match id.rsplit_once('@') {
        Some((n, m)) => (n.to_string(), m.to_string()),
        None => (id.to_string(), String::new()),
    }
}

/// Read `<base>/skills/*/SKILL.md` frontmatter into Skills.
fn scan_skills(base: &Path, owner: &str) -> Vec<Skill> {
    let dir = base.join("skills");
    let rd = match std::fs::read_dir(&dir) {
        Ok(r) => r,
        Err(_) => return Vec::new(),
    };
    let mut skills = Vec::new();
    for entry in rd.flatten() {
        let md = entry.path().join("SKILL.md");
        let text = match std::fs::read_to_string(&md) {
            Ok(t) => t,
            Err(_) => continue,
        };
        let (name, desc) = parse_frontmatter(&text);
        if let Some(name) = name {
            skills.push(Skill {
                name,
                description: desc.unwrap_or_default(),
                owner: owner.to_string(),
            });
        }
    }
    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Σ est_tokens(name + description) over `<install>/{agents,commands}/*.md`.
fn scan_extra_tokens(install_path: &Path) -> u32 {
    let mut total = 0;
    for dir in ["agents", "commands"] {
        let Ok(rd) = std::fs::read_dir(install_path.join(dir)) else {
            continue;
        };
        for entry in rd.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) != Some("md") {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let (name, desc) = parse_frontmatter(&text);
            let name = name
                .or_else(|| path.file_stem().map(|s| s.to_string_lossy().into_owned()))
                .unwrap_or_default();
            total +=
                EXTRA_OVERHEAD + est_tokens(&format!("{}\n{}", name, desc.unwrap_or_default()));
        }
    }
    total
}

/// What the plugin ships, by directory/file presence + plugin.json `lsp` key.
fn detect_provides(install_path: &Path, skills: &[Skill]) -> Vec<String> {
    let mut p = Vec::new();
    if !skills.is_empty() {
        p.push("skills".to_string());
    }
    if install_path.join(".mcp.json").is_file() {
        p.push("mcp".to_string());
    }
    for dir in ["agents", "commands", "hooks"] {
        if install_path.join(dir).is_dir() {
            p.push(dir.to_string());
        }
    }
    if plugin_has_lsp(install_path) {
        p.push("lsp".to_string());
    }
    p
}

fn plugin_has_lsp(install_path: &Path) -> bool {
    let pj = install_path.join(".claude-plugin/plugin.json");
    match std::fs::read_to_string(&pj) {
        Ok(t) => serde_json::from_str::<serde_json::Value>(&t)
            .ok()
            .and_then(|v| v.get("lsp").cloned())
            .is_some(),
        Err(_) => false,
    }
}

/// Hand-split YAML frontmatter; returns (name, description). Handles plain, quoted, block
/// (`>`/`|`) and plain multi-line values for those two keys; other keys are ignored.
// ponytail: not a full YAML parser; add serde_yaml if a skill needs nested keys or anchors.
fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return (None, None);
    }
    let (mut name, mut desc) = (None, None);
    let mut lines = lines.peekable();
    while let Some(line) = lines.next() {
        let t = line.trim_end();
        if t.trim() == "---" {
            break;
        }
        let (key, v) = if let Some(v) = t.strip_prefix("name:") {
            ("name", v)
        } else if let Some(v) = t.strip_prefix("description:") {
            ("description", v)
        } else {
            continue;
        };
        // Indented lines after the key belong to its value (block scalar or plain continuation).
        let mut cont = Vec::new();
        while let Some(next) = lines.peek() {
            if next.trim().is_empty() || next.starts_with(' ') || next.starts_with('\t') {
                cont.push(lines.next().unwrap().trim());
            } else {
                break;
            }
        }
        let v = v.trim();
        let value = if v.starts_with('>') || v.starts_with('|') {
            let sep = if v.starts_with('>') { " " } else { "\n" };
            cont.iter()
                .filter(|l| !l.is_empty())
                .copied()
                .collect::<Vec<_>>()
                .join(sep)
        } else {
            let mut parts = vec![v];
            parts.extend(cont.iter().filter(|l| !l.is_empty()));
            unquote(&parts.join(" "))
        };
        if key == "name" {
            name = Some(value);
        } else {
            desc = Some(value);
        }
    }
    (name, desc)
}

fn unquote(s: &str) -> String {
    let s = s.trim();
    let bytes = s.as_bytes();
    if bytes.len() >= 2
        && ((bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[bytes.len() - 1] == b'\''))
    {
        s[1..s.len() - 1].to_string()
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn parses_frontmatter() {
        let (n, d) = parse_frontmatter("---\nname: foo\ndescription: \"does X\"\n---\nbody");
        assert_eq!(n.as_deref(), Some("foo"));
        assert_eq!(d.as_deref(), Some("does X"));
    }

    #[test]
    fn parses_folded_and_literal_block_descriptions() {
        let (n, d) = parse_frontmatter(
            "---\nname: foo\ndescription: >\n  line one\n  line two\nother: x\n---\nbody",
        );
        assert_eq!(n.as_deref(), Some("foo"));
        assert_eq!(d.as_deref(), Some("line one line two"));
        let (_, d) = parse_frontmatter("---\nname: f\ndescription: |-\n  a\n  b\n---\n");
        assert_eq!(d.as_deref(), Some("a\nb"));
        let (_, d) = parse_frontmatter("---\nname: f\ndescription: plain\n  more\n---\n");
        assert_eq!(d.as_deref(), Some("plain more"));
    }

    #[test]
    fn agents_and_commands_count_toward_cost() {
        let dir = tempfile::tempdir().unwrap();
        fs::create_dir_all(dir.path().join("agents")).unwrap();
        fs::create_dir_all(dir.path().join("commands")).unwrap();
        fs::write(
            dir.path().join("agents/a.md"),
            format!("---\nname: a\ndescription: {}\n---\n", "x".repeat(80)),
        )
        .unwrap();
        fs::write(
            dir.path().join("commands/c.md"),
            format!("---\ndescription: \"{}\"\n---\n", "y".repeat(80)),
        )
        .unwrap();
        assert!(scan_extra_tokens(dir.path()) >= 40);
        assert_eq!(scan_extra_tokens(Path::new("/no/such")), 0);
    }

    #[test]
    fn splits_id_on_last_at() {
        assert_eq!(split_id("vercel@mkt"), ("vercel".into(), "mkt".into()));
        assert_eq!(split_id("a@b@c"), ("a@b".into(), "c".into()));
    }

    fn skill(name: &str, desc: &str) -> Skill {
        Skill {
            name: name.into(),
            description: desc.into(),
            owner: "plugin".into(),
        }
    }

    #[test]
    fn est_tokens_heuristic() {
        assert_eq!(est_tokens(""), 0);
        assert!(est_tokens("a longer string here") > est_tokens("short"));
        // ~600 chars should land in the 100–200 tok ballpark (claude's per-skill range).
        let n = est_tokens(&"x".repeat(600));
        assert!((100..=200).contains(&n), "est {n} out of ballpark");
    }

    #[test]
    fn plugin_cost_sums_skills_and_zero_when_empty() {
        let empty = Plugin {
            id: "e@m".into(),
            name: "e".into(),
            marketplace: "m".into(),
            version: "1".into(),
            install_path: Default::default(),
            provides: vec![],
            skills: vec![],
            extra_tokens: 0,
        };
        assert_eq!(empty.est_tokens(), 0);

        let two = Plugin {
            skills: vec![skill("a", &"x".repeat(80)), skill("b", &"y".repeat(80))],
            ..empty.clone()
        };
        let expected = 2 * SKILL_OVERHEAD
            + est_tokens(&format!("a\n{}", "x".repeat(80)))
            + est_tokens(&format!("b\n{}", "y".repeat(80)));
        assert_eq!(two.est_tokens(), expected);
        assert!(two.est_tokens() > 0);
    }

    #[test]
    fn loads_user_scope_plugin_with_skills_and_provides() {
        let home = tempfile::tempdir().unwrap();
        let install = home.path().join("install/demo/1.0.0");
        fs::create_dir_all(install.join("skills/foo")).unwrap();
        fs::create_dir_all(install.join("agents")).unwrap();
        fs::write(
            install.join("skills/foo/SKILL.md"),
            "---\nname: foo\ndescription: a skill\n---\nbody",
        )
        .unwrap();
        fs::write(install.join(".mcp.json"), "{}").unwrap();

        let pdir = home.path().join(".claude/plugins");
        fs::create_dir_all(&pdir).unwrap();
        let json = serde_json::json!({
            "version": 2,
            "plugins": {
                "demo@mkt": [
                    {"scope": "local", "installPath": "/wrong"},
                    {"scope": "user", "installPath": install.to_str().unwrap(), "version": "1.0.0"}
                ]
            }
        });
        fs::write(pdir.join("installed_plugins.json"), json.to_string()).unwrap();

        let plugins = load(home.path()).unwrap();
        assert_eq!(plugins.len(), 1);
        let p = &plugins[0];
        assert_eq!(p.id, "demo@mkt");
        assert_eq!(p.name, "demo");
        assert_eq!(p.marketplace, "mkt");
        assert_eq!(p.install_path, install);
        assert_eq!(p.skills.len(), 1);
        assert_eq!(p.skills[0].name, "foo");
        assert!(p.provides.contains(&"skills".to_string()));
        assert!(p.provides.contains(&"mcp".to_string()));
        assert!(p.provides.contains(&"agents".to_string()));
    }

    #[test]
    fn skips_plugin_without_user_record() {
        let home = tempfile::tempdir().unwrap();
        let pdir = home.path().join(".claude/plugins");
        fs::create_dir_all(&pdir).unwrap();
        let json = serde_json::json!({
            "version": 2,
            "plugins": { "x@m": [ {"scope": "local", "installPath": "/p"} ] }
        });
        fs::write(pdir.join("installed_plugins.json"), json.to_string()).unwrap();
        assert!(load(home.path()).unwrap().is_empty());
    }
}

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
}

#[derive(Debug, Clone, Serialize)]
pub struct Skill {
    pub name: String,
    pub description: String,
    pub owner: String, // "plugin" | "loose"
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

/// Hand-split YAML frontmatter; returns (name, description). Flat keys only.
// ponytail: flat frontmatter only; add serde_yaml if a skill needs nested/multiline keys.
fn parse_frontmatter(text: &str) -> (Option<String>, Option<String>) {
    let mut lines = text.lines();
    if lines.next().map(|l| l.trim()) != Some("---") {
        return (None, None);
    }
    let (mut name, mut desc) = (None, None);
    for line in lines {
        let t = line.trim_end();
        if t.trim() == "---" {
            break;
        }
        if let Some(v) = t.strip_prefix("name:") {
            name = Some(unquote(v.trim()));
        } else if let Some(v) = t.strip_prefix("description:") {
            desc = Some(unquote(v.trim()));
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
    fn splits_id_on_last_at() {
        assert_eq!(split_id("vercel@mkt"), ("vercel".into(), "mkt".into()));
        assert_eq!(split_id("a@b@c"), ("a@b".into(), "c".into()));
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

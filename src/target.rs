use crate::inventory::{Plugin, Skill};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Plugin,
    PluginGlob,
    PluginSkill,
    LooseSkill,
}

/// One resolved target. `ok == false` means it cannot be applied (unknown, ambiguous,
/// or plugin-owned-skill which is unsupported in v1); the batch continues regardless.
#[derive(Debug, Clone, Serialize)]
pub struct Resolved {
    pub raw: String,
    pub kind: Kind,
    pub plugin_id: Option<String>,
    pub skill: Option<String>,
    pub ok: bool,
    pub reason: Option<String>,
}

impl Resolved {
    fn bad(raw: &str, kind: Kind, reason: &str) -> Self {
        Resolved {
            raw: raw.to_string(),
            kind,
            plugin_id: None,
            skill: None,
            ok: false,
            reason: Some(reason.to_string()),
        }
    }
}

/// Resolve a raw target string against the inventory.
pub fn resolve(raw: &str, plugins: &[Plugin], loose: &[Skill]) -> Resolved {
    let (left, right) = match raw.split_once(':') {
        Some((l, r)) => (l, Some(r)),
        None => (raw, None),
    };

    match right {
        // bare plugin
        None => match resolve_plugin(left, plugins) {
            Ok(id) => Resolved {
                raw: raw.to_string(),
                kind: Kind::Plugin,
                plugin_id: Some(id),
                skill: None,
                ok: true,
                reason: None,
            },
            Err(reason) => Resolved::bad(raw, Kind::Plugin, reason),
        },

        // glob: plugin:*
        Some("*") => match resolve_plugin(left, plugins) {
            Ok(id) => Resolved {
                raw: raw.to_string(),
                kind: Kind::PluginGlob,
                plugin_id: Some(id),
                skill: None,
                ok: true,
                reason: None,
            },
            Err(reason) => Resolved::bad(raw, Kind::PluginGlob, reason),
        },

        // plugin:skill
        Some(skill) => {
            // If `left` names a plugin that owns `skill`, it's a plugin-owned skill
            // (unsupported in v1). Otherwise fall through to the loose-skill lookup.
            if let Ok(id) = resolve_plugin(left, plugins) {
                let owns = plugins
                    .iter()
                    .find(|p| p.id == id)
                    .map(|p| p.skills.iter().any(|s| s.name == skill))
                    .unwrap_or(false);
                if owns {
                    return Resolved {
                        raw: raw.to_string(),
                        kind: Kind::PluginSkill,
                        plugin_id: Some(id),
                        skill: Some(skill.to_string()),
                        ok: false,
                        reason: Some("unsupported-v1".to_string()),
                    };
                }
            }

            if loose.iter().any(|s| s.name == skill) {
                Resolved {
                    raw: raw.to_string(),
                    kind: Kind::LooseSkill,
                    plugin_id: None,
                    skill: Some(skill.to_string()),
                    ok: true,
                    reason: None,
                }
            } else {
                Resolved::bad(raw, Kind::PluginSkill, "unknown-skill")
            }
        }
    }
}

/// Resolve a plugin reference (bare `name` or full `name@marketplace`) to a full id.
fn resolve_plugin(reference: &str, plugins: &[Plugin]) -> Result<String, &'static str> {
    if reference.contains('@') {
        return plugins
            .iter()
            .find(|p| p.id == reference)
            .map(|p| p.id.clone())
            .ok_or("unknown-plugin");
    }
    let matches: Vec<&Plugin> = plugins.iter().filter(|p| p.name == reference).collect();
    match matches.as_slice() {
        [] => Err("unknown-plugin"),
        [p] => Ok(p.id.clone()),
        _ => Err("ambiguous"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plugin(id: &str, skills: &[&str]) -> Plugin {
        let (name, marketplace) = id.rsplit_once('@').unwrap();
        Plugin {
            id: id.to_string(),
            name: name.to_string(),
            marketplace: marketplace.to_string(),
            version: "1".into(),
            install_path: Default::default(),
            provides: vec![],
            skills: skills
                .iter()
                .map(|s| Skill {
                    name: s.to_string(),
                    description: String::new(),
                    owner: "plugin".into(),
                })
                .collect(),
        }
    }

    fn fixture() -> Vec<Plugin> {
        vec![
            plugin("vercel@official", &["nextjs", "deploy"]),
            plugin("dup@one", &[]),
            plugin("dup@two", &[]),
        ]
    }

    #[test]
    fn bare_name_resolves() {
        let r = resolve("vercel", &fixture(), &[]);
        assert_eq!(r.kind, Kind::Plugin);
        assert!(r.ok);
        assert_eq!(r.plugin_id.as_deref(), Some("vercel@official"));
    }

    #[test]
    fn full_id_accepted() {
        let r = resolve("vercel@official", &fixture(), &[]);
        assert!(r.ok && r.plugin_id.as_deref() == Some("vercel@official"));
    }

    #[test]
    fn ambiguous_rejected() {
        let r = resolve("dup", &fixture(), &[]);
        assert!(!r.ok);
        assert_eq!(r.reason.as_deref(), Some("ambiguous"));
    }

    #[test]
    fn unknown_plugin_rejected() {
        let r = resolve("nope", &fixture(), &[]);
        assert_eq!(r.reason.as_deref(), Some("unknown-plugin"));
    }

    #[test]
    fn plugin_owned_skill_is_unsupported_v1() {
        let r = resolve("vercel:nextjs", &fixture(), &[]);
        assert_eq!(r.kind, Kind::PluginSkill);
        assert!(!r.ok);
        assert_eq!(r.reason.as_deref(), Some("unsupported-v1"));
    }

    #[test]
    fn loose_skill_matched() {
        let loose = vec![Skill {
            name: "myloose".into(),
            description: String::new(),
            owner: "loose".into(),
        }];
        let r = resolve("anything:myloose", &fixture(), &loose);
        assert_eq!(r.kind, Kind::LooseSkill);
        assert!(r.ok);
        assert_eq!(r.skill.as_deref(), Some("myloose"));
    }

    #[test]
    fn glob_classified() {
        let r = resolve("vercel:*", &fixture(), &[]);
        assert_eq!(r.kind, Kind::PluginGlob);
        assert!(r.ok);
        assert_eq!(r.plugin_id.as_deref(), Some("vercel@official"));
    }
}

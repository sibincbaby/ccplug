use crate::cli::Scope;
use anyhow::{Context, Result};
use serde_json::{Map, Value};
use std::path::{Path, PathBuf};

/// Map a scope to its settings file path.
pub fn scope_path(scope: Scope, project: &Path, home: &Path) -> PathBuf {
    match scope {
        Scope::Project => project.join(".claude/settings.json"),
        Scope::Local => project.join(".claude/settings.local.json"),
        Scope::User => home.join(".claude/settings.json"),
    }
}

/// Read a settings file as a JSON object. Missing file → empty object.
pub fn read_value(path: &Path) -> Result<Value> {
    match std::fs::read_to_string(path) {
        Ok(t) => {
            let v: Value =
                serde_json::from_str(&t).with_context(|| format!("parsing {}", path.display()))?;
            Ok(v)
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Value::Object(Map::new())),
        Err(e) => Err(e).with_context(|| format!("reading {}", path.display())),
    }
}

/// Effective state after the cascade (user < project < local), per-key, with the winning source.
#[derive(Debug, Default)]
pub struct Effective {
    pub plugins: Vec<(String, bool, Scope)>, // sorted by id
    pub overrides: Vec<(String, String)>,    // skill name → on/name-only/off
}

pub fn effective(project: &Path, home: &Path) -> Result<Effective> {
    use std::collections::BTreeMap;
    let mut plugins: BTreeMap<String, (bool, Scope)> = BTreeMap::new();
    let mut overrides: BTreeMap<String, String> = BTreeMap::new();

    for scope in [Scope::User, Scope::Project, Scope::Local] {
        let v = read_value(&scope_path(scope, project, home))?;
        if let Some(ep) = v.get("enabledPlugins").and_then(|x| x.as_object()) {
            for (k, val) in ep {
                if let Some(b) = val.as_bool() {
                    plugins.insert(k.clone(), (b, scope));
                }
            }
        }
        if let Some(so) = v.get("skillOverrides").and_then(|x| x.as_object()) {
            for (k, val) in so {
                if let Some(s) = val.as_str() {
                    overrides.insert(k.clone(), s.to_string());
                }
            }
        }
    }

    Ok(Effective {
        plugins: plugins.into_iter().map(|(k, (b, s))| (k, b, s)).collect(),
        overrides: overrides.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn scope_paths() {
        let p = Path::new("/proj");
        let h = Path::new("/home");
        assert_eq!(
            scope_path(Scope::Project, p, h),
            Path::new("/proj/.claude/settings.json")
        );
        assert_eq!(
            scope_path(Scope::Local, p, h),
            Path::new("/proj/.claude/settings.local.json")
        );
        assert_eq!(
            scope_path(Scope::User, p, h),
            Path::new("/home/.claude/settings.json")
        );
    }

    #[test]
    fn missing_file_is_empty_object() {
        let v = read_value(Path::new("/no/such/file.json")).unwrap();
        assert!(v.as_object().unwrap().is_empty());
    }

    #[test]
    fn cascade_local_overrides_project_overrides_user() {
        let home = tempfile::tempdir().unwrap();
        let proj = tempfile::tempdir().unwrap();
        fs::create_dir_all(home.path().join(".claude")).unwrap();
        fs::create_dir_all(proj.path().join(".claude")).unwrap();
        fs::write(
            home.path().join(".claude/settings.json"),
            r#"{"enabledPlugins":{"a@m":true,"b@m":true}}"#,
        )
        .unwrap();
        fs::write(
            proj.path().join(".claude/settings.json"),
            r#"{"enabledPlugins":{"b@m":false}}"#,
        )
        .unwrap();
        fs::write(
            proj.path().join(".claude/settings.local.json"),
            r#"{"skillOverrides":{"deploy":"off"}}"#,
        )
        .unwrap();

        let eff = effective(proj.path(), home.path()).unwrap();
        let b = eff.plugins.iter().find(|(k, ..)| k == "b@m").unwrap();
        assert!(!b.1, "project should override user");
        assert_eq!(b.2, Scope::Project);
        let a = eff.plugins.iter().find(|(k, ..)| k == "a@m").unwrap();
        assert_eq!(a.2, Scope::User);
        assert_eq!(eff.overrides, vec![("deploy".into(), "off".into())]);
    }
}

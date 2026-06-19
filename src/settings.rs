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

/// A set of toggles to apply to one settings file.
#[derive(Debug, Default)]
pub struct Change {
    pub set_plugin: Vec<(String, bool)>, // enabledPlugins[id] = bool
    pub set_override: Vec<(String, String)>, // skillOverrides[name] = "on"/"off"
}

#[derive(Debug)]
pub struct Applied {
    pub wrote: bool,
    pub backup: Option<PathBuf>,
    pub after: String,
}

/// Read-modify-write a single settings file, touching only `enabledPlugins`/`skillOverrides`.
/// Preserves every other key (and key order, via serde_json `preserve_order`). Never rewrites
/// wholesale. Backs up to `<path>.bak` once before the first real write; `dry_run` writes nothing.
// ponytail: serde_json reformats (drops comments / exact whitespace) but preserves all keys and
// order — acceptable for settings.json; upgrade to a JSONC-preserving editor only if a user keeps
// comments in these files.
pub fn apply(path: &Path, change: &Change, dry_run: bool) -> Result<Applied> {
    let existed = path.exists();
    let original = read_value(path)?;
    let before = serde_json::to_string_pretty(&original)?;

    let mut next = original.clone();
    let obj = next
        .as_object_mut()
        .ok_or_else(|| anyhow::anyhow!("{} is not a JSON object", path.display()))?;

    if !change.set_plugin.is_empty() {
        let ep = obj
            .entry("enabledPlugins")
            .or_insert_with(|| Value::Object(Map::new()));
        let m = ep
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("enabledPlugins is not an object"))?;
        for (id, on) in &change.set_plugin {
            m.insert(id.clone(), Value::Bool(*on));
        }
    }
    if !change.set_override.is_empty() {
        let so = obj
            .entry("skillOverrides")
            .or_insert_with(|| Value::Object(Map::new()));
        let m = so
            .as_object_mut()
            .ok_or_else(|| anyhow::anyhow!("skillOverrides is not an object"))?;
        for (name, state) in &change.set_override {
            m.insert(name.clone(), Value::String(state.clone()));
        }
    }

    let after = serde_json::to_string_pretty(&next)?;
    let changed = after != before;

    let mut backup = None;
    let mut wrote = false;
    if changed && !dry_run {
        if existed {
            let bak = path.with_extension("json.bak");
            std::fs::copy(path, &bak).with_context(|| format!("backing up {}", path.display()))?;
            backup = Some(bak);
        } else if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        std::fs::write(path, format!("{after}\n"))
            .with_context(|| format!("writing {}", path.display()))?;
        wrote = true;
    }

    Ok(Applied {
        wrote,
        backup,
        after,
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

    #[test]
    fn apply_preserves_unrelated_keys_and_backs_up() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"$schema":"x","permissions":{"allow":["Bash(ls)"]},"enabledPlugins":{"keep@m":true}}"#,
        )
        .unwrap();

        let change = Change {
            set_plugin: vec![("vercel@m".into(), false)],
            set_override: vec![("deploy".into(), "off".into())],
        };
        let applied = apply(&path, &change, false).unwrap();
        assert!(applied.wrote);
        assert!(applied.backup.unwrap().exists());

        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["$schema"], "x"); // unrelated key intact
        assert_eq!(v["permissions"]["allow"][0], "Bash(ls)");
        assert_eq!(v["enabledPlugins"]["keep@m"], true); // existing toggle intact
        assert_eq!(v["enabledPlugins"]["vercel@m"], false); // new toggle applied
        assert_eq!(v["skillOverrides"]["deploy"], "off");
    }

    #[test]
    fn apply_creates_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join(".claude/settings.json");
        let change = Change {
            set_plugin: vec![("x@m".into(), true)],
            ..Default::default()
        };
        let applied = apply(&path, &change, false).unwrap();
        assert!(applied.wrote);
        assert!(applied.backup.is_none());
        let v: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["enabledPlugins"]["x@m"], true);
    }

    #[test]
    fn dry_run_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let change = Change {
            set_plugin: vec![("x@m".into(), true)],
            ..Default::default()
        };
        let applied = apply(&path, &change, true).unwrap();
        assert!(!applied.wrote);
        assert!(!path.exists());
        assert!(applied.after.contains("x@m"));
    }
}

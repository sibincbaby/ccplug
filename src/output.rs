use crate::inventory::Plugin;
use anyhow::Result;
use comfy_table::presets::UTF8_BORDERS_ONLY;
use comfy_table::Table;

/// `ccplug list` — JSON per spec §6, with effective `enabled` injected per plugin.
pub fn list_json(plugins: &[Plugin], enabled: &dyn Fn(&str) -> bool) -> Result<String> {
    let arr: Vec<serde_json::Value> = plugins
        .iter()
        .map(|p| {
            let mut v = serde_json::to_value(p).unwrap();
            v["enabled"] = serde_json::Value::Bool(enabled(&p.id));
            v
        })
        .collect();
    Ok(serde_json::to_string_pretty(
        &serde_json::json!({ "plugins": arr }),
    )?)
}

/// `ccplug list` — human table: one row per plugin, skills summarized.
pub fn list_table(plugins: &[Plugin], enabled: &dyn Fn(&str) -> bool) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(vec!["PLUGIN", "ON", "PROVIDES", "SKILLS"]);
    for p in plugins {
        let skills = p
            .skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        table.add_row(vec![
            p.id.clone(),
            if enabled(&p.id) {
                "✓".into()
            } else {
                "·".into()
            },
            p.provides.join(","),
            truncate(&skills, 60),
        ]);
    }
    format!("{table}\n{} plugins", plugins.len())
}

pub fn truncate(s: &str, max: usize) -> String {
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= max {
        return s.to_string();
    }
    let head: String = chars[..max.saturating_sub(1)].iter().collect();
    format!("{head}…")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::inventory::Skill;

    fn sample() -> Plugin {
        Plugin {
            id: "vercel@official".into(),
            name: "vercel".into(),
            marketplace: "official".into(),
            version: "1".into(),
            install_path: Default::default(),
            provides: vec!["skills".into(), "mcp".into()],
            skills: vec![Skill {
                name: "nextjs".into(),
                description: "x".into(),
                owner: "plugin".into(),
            }],
        }
    }

    #[test]
    fn json_has_plugins_key() {
        let out = list_json(&[sample()], &|_| true).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["plugins"][0]["name"], "vercel");
        assert_eq!(v["plugins"][0]["skills"][0]["owner"], "plugin");
    }

    #[test]
    fn table_marks_enabled() {
        let out = list_table(&[sample()], &|id| id == "vercel@official");
        assert!(out.contains("vercel@official"));
        assert!(out.contains("✓"));
        assert!(out.contains("1 plugins"));
    }
}

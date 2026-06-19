use crate::inventory::Plugin;
use anyhow::Result;
use comfy_table::presets::UTF8_BORDERS_ONLY;
use comfy_table::Table;

/// `ccplug list` — JSON per spec §6, with effective `enabled` + `estTokens` per plugin
/// and a `summary` of total / enabled estimated cost.
pub fn list_json(plugins: &[Plugin], enabled: &dyn Fn(&str) -> bool) -> Result<String> {
    let mut total = 0u32;
    let mut enabled_total = 0u32;
    let arr: Vec<serde_json::Value> = plugins
        .iter()
        .map(|p| {
            let cost = p.est_tokens();
            total += cost;
            if enabled(&p.id) {
                enabled_total += cost;
            }
            let mut v = serde_json::to_value(p).unwrap();
            v["enabled"] = serde_json::Value::Bool(enabled(&p.id));
            v["estTokens"] = serde_json::Value::from(cost);
            v
        })
        .collect();
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "plugins": arr,
        "summary": { "totalEst": total, "enabledEst": enabled_total },
    }))?)
}

/// `ccplug list` — human table with a COST column and total/enabled cost footer.
pub fn list_table(plugins: &[Plugin], enabled: &dyn Fn(&str) -> bool) -> String {
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY);
    table.set_header(vec!["PLUGIN", "ON", "COST (tok)", "PROVIDES", "SKILLS"]);
    let (mut total, mut enabled_total) = (0u32, 0u32);
    for p in plugins {
        let cost = p.est_tokens();
        total += cost;
        let on = enabled(&p.id);
        if on {
            enabled_total += cost;
        }
        let skills = p
            .skills
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        table.add_row(vec![
            p.id.clone(),
            if on { "✓".into() } else { "·".into() },
            format!("~{cost}"),
            p.provides.join(","),
            truncate(&skills, 60),
        ]);
    }
    format!(
        "{table}\n{} plugins | cost ~{enabled_total} tok enabled / ~{total} tok all\n\
         cost = est. always-on tokens from skill descriptions; exact: claude plugin details <name>",
        plugins.len()
    )
}

/// One per-target outcome for `enable`/`disable` (spec §6).
#[derive(serde::Serialize)]
pub struct TargetResult {
    pub target: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub action: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// `enable`/`disable` — JSON result per spec §6.
pub fn mutate_json(
    file: &str,
    scope: &str,
    dry_run: bool,
    results: &[TargetResult],
    warnings: &[String],
) -> Result<String> {
    Ok(serde_json::to_string_pretty(&serde_json::json!({
        "file": file,
        "scope": scope,
        "dryRun": dry_run,
        "results": results,
        "warnings": warnings,
    }))?)
}

/// `enable`/`disable` — human summary.
pub fn mutate_human(
    file: &str,
    dry_run: bool,
    results: &[TargetResult],
    warnings: &[String],
    diff: Option<&str>,
    wrote: bool,
    backup: Option<&str>,
) -> String {
    let mut out = String::new();
    for r in results {
        let mark = if r.ok { "✓" } else { "✗" };
        let detail = r
            .action
            .clone()
            .or_else(|| r.reason.clone())
            .unwrap_or_default();
        out.push_str(&format!("{mark} {} ({}) {detail}\n", r.target, r.kind));
    }
    for w in warnings {
        out.push_str(&format!("! {w}\n"));
    }
    if dry_run {
        out.push_str(&format!("dry-run: no changes written to {file}\n"));
        if let Some(d) = diff {
            out.push_str(d);
            out.push('\n');
        }
    } else if wrote {
        out.push_str(&format!("wrote {file}\n"));
        if let Some(b) = backup {
            out.push_str(&format!("backup: {b}\n"));
        }
    } else {
        out.push_str(&format!("no changes ({file} already up to date)\n"));
    }
    out.trim_end().to_string()
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

    #[test]
    fn json_summary_counts_only_enabled() {
        let mut on = sample(); // enabled
        on.id = "on@m".into();
        let mut off = sample(); // disabled
        off.id = "off@m".into();
        let cost = on.est_tokens();
        assert!(cost > 0);

        let out = list_json(&[on.clone(), off], &|id| id == "on@m").unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["plugins"][0]["estTokens"], cost);
        assert_eq!(v["summary"]["totalEst"], cost * 2);
        assert_eq!(v["summary"]["enabledEst"], cost); // only the enabled one
    }

    #[test]
    fn table_shows_cost_column_and_footer() {
        let out = list_table(&[sample()], &|_| true);
        assert!(out.contains("COST"));
        assert!(out.contains(&format!("~{}", sample().est_tokens())));
        assert!(out.contains("tok enabled"));
    }
}

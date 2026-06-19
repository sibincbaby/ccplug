use clap::{Args, Parser, Subcommand, ValueEnum};

#[derive(Parser, Debug)]
#[command(
    name = "ccplug",
    version,
    about = "Manage which Claude Code plugins and skills are enabled per project"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// List every global plugin, its skills, and enabled state
    List(CommonFlags),
    /// Show what is effectively active in the current project after the cascade
    Status(CommonFlags),
    /// Enable plugins/skills for the chosen scope
    Enable(MutateArgs),
    /// Disable plugins/skills for the chosen scope
    Disable(MutateArgs),
}

#[derive(Args, Debug, Clone)]
pub struct CommonFlags {
    /// Machine-readable JSON output
    #[arg(long)]
    pub json: bool,

    /// Settings scope to read/write (default: project)
    #[arg(long, value_enum, default_value_t = Scope::Project)]
    pub scope: Scope,

    /// Override ~/.claude location (for tests)
    #[arg(long, hide = true)]
    pub home_dir: Option<String>,

    /// Override the project directory (default: cwd; for tests)
    #[arg(long, hide = true)]
    pub project_dir: Option<String>,
}

#[derive(Args, Debug, Clone)]
pub struct MutateArgs {
    /// Targets: `plugin` | `plugin:skill` | `plugin:*`
    pub targets: Vec<String>,

    #[command(flatten)]
    pub common: CommonFlags,

    /// Read targets as a JSON array from FILE, e.g. ["vercel","superpowers:*"]
    #[arg(long)]
    pub from: Option<String>,

    /// Read targets as a JSON array from stdin
    #[arg(long)]
    pub stdin: bool,

    /// Print the diff, write nothing
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(ValueEnum, Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Project,
    Local,
    User,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Scope::Project => "project",
            Scope::Local => "local",
            Scope::User => "user",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    fn parse(args: &[&str]) -> Command {
        Cli::parse_from(args).command
    }

    #[test]
    fn parses_all_subcommands() {
        assert!(matches!(
            parse(&["ccplug", "list", "--json"]),
            Command::List(_)
        ));
        assert!(matches!(parse(&["ccplug", "status"]), Command::Status(_)));
        assert!(matches!(
            parse(&["ccplug", "enable", "a", "b", "--dry-run"]),
            Command::Enable(_)
        ));
        assert!(matches!(
            parse(&["ccplug", "disable", "--stdin"]),
            Command::Disable(_)
        ));
    }

    #[test]
    fn scope_defaults_to_project_and_parses_user() {
        if let Command::List(f) = parse(&["ccplug", "list"]) {
            assert_eq!(f.scope, Scope::Project);
        } else {
            panic!("expected list");
        }
        if let Command::Disable(a) = parse(&["ccplug", "disable", "x", "--scope", "user"]) {
            assert_eq!(a.common.scope, Scope::User);
        } else {
            panic!("expected disable");
        }
    }

    #[test]
    fn enable_collects_positional_targets() {
        if let Command::Enable(a) = parse(&["ccplug", "enable", "vercel", "ponytail:*"]) {
            assert_eq!(a.targets, vec!["vercel", "ponytail:*"]);
        } else {
            panic!("expected enable");
        }
    }
}

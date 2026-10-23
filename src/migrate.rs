//! Moves a thefuck setup over to welp: the shell config lines and the
//! settings file. Every file it changes is backed up first.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use regex::Regex;

use crate::config::Config;
use crate::shells::Shell;

#[derive(Debug, Default)]
pub struct Report {
    /// Shell config files that were (or would be) changed.
    pub shell_files: Vec<PathBuf>,
    pub alias: Option<String>,
    pub settings_imported: bool,
}

static ALIAS_FLAG: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"--alias\s+([A-Za-z0-9_-]+)").unwrap());
const MARK: &str = "# disabled by welp migrate: ";

/// `home` is the user's home folder; `config` is where welp's settings go.
pub fn migrate(home: &Path, config: &Path, dry_run: bool) -> std::io::Result<Report> {
    let mut report = Report::default();

    for shell in Shell::ALL {
        for name in shell.config_files() {
            let path = home.join(name);
            let Ok(text) = fs::read_to_string(&path) else {
                continue;
            };
            let Some((new_text, alias)) = rewrite(&text, shell) else {
                continue;
            };
            if !dry_run {
                fs::write(backup_path(&path), &text)?;
                fs::write(&path, new_text)?;
            }
            report.alias = Some(alias);
            report.shell_files.push(path);
        }
    }

    let settings = home.join(".config/thefuck/settings.py");
    if let Ok(text) = fs::read_to_string(settings)
        && !config.exists()
    {
        if !dry_run {
            fs::create_dir_all(config.parent().unwrap_or(home))?;
            let imported = import_settings(&text);
            fs::write(
                config,
                toml::to_string_pretty(&imported).expect("config serializes"),
            )?;
        }
        report.settings_imported = true;
    }
    Ok(report)
}

fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".welp-backup");
    PathBuf::from(name)
}

/// Comments out the thefuck lines and adds welp's under the last of them,
/// keeping the alias the user typed. `None` when there is nothing to change.
fn rewrite(text: &str, shell: Shell) -> Option<(String, String)> {
    let is_thefuck = |line: &str| line.contains("thefuck") && !line.trim_start().starts_with('#');
    let last = text
        .lines()
        .collect::<Vec<_>>()
        .iter()
        .rposition(|line| is_thefuck(line))?;
    let alias = text
        .lines()
        .filter(|line| is_thefuck(line))
        .find_map(|line| ALIAS_FLAG.captures(line).map(|found| found[1].to_string()))
        .unwrap_or_else(|| "fuck".to_string());

    let mut lines = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if is_thefuck(line) {
            lines.push(format!("{MARK}{line}"));
        } else {
            lines.push(line.to_string());
        }
        if index == last && !text.contains("welp init") {
            lines.push(shell.init_line(&alias));
        }
    }
    let mut new_text = lines.join("\n");
    if text.ends_with('\n') {
        new_text.push('\n');
    }
    Some((new_text, alias))
}

/// Reads thefuck's settings.py, which is Python but in practice plain
/// `name = value` lines.
fn import_settings(text: &str) -> Config {
    let mut config = Config::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let (key, value) = (key.trim(), value.trim());
        match key {
            "require_confirmation" => config.require_confirmation = value == "True",
            "wait_command" => config.wait_command = value.parse().unwrap_or(config.wait_command),
            "wait_slow_command" => {
                config.wait_slow_command = value.parse().unwrap_or(config.wait_slow_command)
            }
            "exclude_rules" => config.exclude_rules = python_list(value),
            "slow_commands" => config.slow_commands = python_list(value),
            // `rules = DEFAULT_RULES + ['rm_root']` turns extra rules on.
            "rules" => {
                config.enable_rules = python_list(
                    value
                        .trim_start_matches("DEFAULT_RULES")
                        .trim_start_matches([' ', '+']),
                )
            }
            _ => {}
        }
    }
    config
}

fn python_list(value: &str) -> Vec<String> {
    value
        .trim_matches(|c| c == '[' || c == ']')
        .split(',')
        .map(|item| {
            item.trim()
                .trim_matches(|c| c == '\'' || c == '"')
                .to_string()
        })
        .filter(|item| !item.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rewrites_zshrc_and_keeps_alias() {
        let home = tempfile::tempdir().unwrap();
        let zshrc = home.path().join(".zshrc");
        fs::write(
            &zshrc,
            "export A=1\neval $(thefuck --alias oops)\nalias ll='ls -l'\n",
        )
        .unwrap();

        let report = migrate(home.path(), &home.path().join("welp.toml"), false).unwrap();
        let text = fs::read_to_string(&zshrc).unwrap();
        assert_eq!(report.alias.as_deref(), Some("oops"));
        assert_eq!(
            text,
            "export A=1\n# disabled by welp migrate: eval $(thefuck --alias oops)\neval \"$(welp init zsh --alias oops)\"\nalias ll='ls -l'\n"
        );
        assert!(home.path().join(".zshrc.welp-backup").exists());

        // Running it twice changes nothing more.
        migrate(home.path(), &home.path().join("welp.toml"), false).unwrap();
        assert_eq!(fs::read_to_string(&zshrc).unwrap(), text);
    }

    #[test]
    fn fish_defaults_to_fuck() {
        let fixed = rewrite("thefuck --alias | source\n", Shell::Fish).unwrap();
        assert_eq!(
            fixed.0,
            "# disabled by welp migrate: thefuck --alias | source\nwelp init fish --alias fuck | source\n"
        );
    }

    #[test]
    fn dry_run_writes_nothing() {
        let home = tempfile::tempdir().unwrap();
        let bashrc = home.path().join(".bashrc");
        fs::write(&bashrc, "eval \"$(thefuck --alias)\"\n").unwrap();
        let report = migrate(home.path(), &home.path().join("welp.toml"), true).unwrap();
        assert_eq!(report.shell_files.len(), 1);
        assert_eq!(
            fs::read_to_string(&bashrc).unwrap(),
            "eval \"$(thefuck --alias)\"\n"
        );
    }

    #[test]
    fn imports_settings() {
        let config = import_settings(
            "require_confirmation = False\nwait_command = 5\nexclude_rules = ['sudo', \"git_push\"]\n",
        );
        assert!(!config.require_confirmation);
        assert_eq!(config.wait_command, 5);
        assert_eq!(config.exclude_rules, ["sudo", "git_push"]);
        let extra = import_settings("rules = DEFAULT_RULES + ['rm_root']\n");
        assert_eq!(extra.enable_rules, ["rm_root"]);
    }
}

//! Rules that turn a failed command into fixes. Each one is a port of the
//! thefuck rule of the same name (MIT, https://github.com/nvbn/thefuck),
//! so `exclude_rules` from a thefuck config keeps working.

mod files;
mod git;
mod system;
mod typos;

use crate::command::Command;
use crate::config::Config;

pub struct Rule {
    pub name: &'static str,
    pub description: &'static str,
    /// Lower runs first; its fixes are offered first.
    pub priority: u32,
    pub matches: fn(&Command) -> bool,
    pub fixes: fn(&Command) -> Vec<String>,
}

pub fn all() -> Vec<Rule> {
    let mut rules = Vec::new();
    rules.extend(git::rules());
    rules.extend(files::rules());
    rules.extend(system::rules());
    rules.extend(typos::rules());
    rules.sort_by_key(|rule| rule.priority);
    rules
}

/// Every fix the enabled rules offer, best first, without repeats.
pub fn suggest(command: &Command, config: &Config) -> Vec<String> {
    let mut fixes: Vec<String> = Vec::new();
    for rule in all() {
        if config.exclude_rules.iter().any(|name| name == rule.name) || !(rule.matches)(command) {
            continue;
        }
        for fix in (rule.fixes)(command) {
            let fix = fix.trim().to_string();
            if !fix.is_empty() && fix != command.script && !fixes.contains(&fix) {
                fixes.push(fix);
            }
        }
    }
    fixes
}

#[cfg(test)]
pub(crate) fn fixes_for(script: &str, output: &str) -> Vec<String> {
    suggest(&Command::new(script, output), &Config::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rule_names_are_unique() {
        let mut names: Vec<_> = all().iter().map(|rule| rule.name).collect();
        names.sort();
        let count = names.len();
        names.dedup();
        assert_eq!(names.len(), count);
    }

    #[test]
    fn excluded_rules_are_skipped() {
        let config = Config {
            exclude_rules: vec!["sl_ls".into()],
            ..Config::default()
        };
        assert!(suggest(&Command::new("sl", ""), &config).is_empty());
    }
}

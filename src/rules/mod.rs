//! Rules that turn a failed command into fixes. Each one is a port of the
//! thefuck rule of the same name (MIT, https://github.com/nvbn/thefuck),
//! so `exclude_rules` from a thefuck config keeps working.

mod files;
mod git;
mod git_extra;
mod system;
mod tools;
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
    rules.extend(git_extra::rules());
    rules.extend(files::rules());
    rules.extend(system::rules());
    rules.extend(tools::rules());
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

/// The script with the argument `old` swapped for `new`, everything else as
/// typed: the last word when it ends the script, otherwise the first one in
/// the middle. Unlike `Command::replace_word` it does not re-quote anything,
/// so globs and `&&` survive.
pub(crate) fn replace_arg(script: &str, old: &str, new: &str) -> String {
    if let Some(head) = script.strip_suffix(old)
        && head.ends_with(' ')
    {
        return format!("{head}{new}");
    }
    script.replacen(&format!(" {old} "), &format!(" {new} "), 1)
}

/// The script with its program (after any `sudo`) swapped for `new`.
pub(crate) fn replace_program(script: &str, new: &str) -> String {
    let (head, rest) = match script.strip_prefix("sudo ") {
        Some(rest) => ("sudo ", rest.trim_start()),
        None => ("", script),
    };
    let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
    format!("{head}{new}{}", &rest[end..])
}

#[cfg(test)]
pub(crate) fn fixes_for(script: &str, output: &str) -> Vec<String> {
    suggest(&Command::new(script, output), &Config::default())
}

/// Whether `fix` is among the fixes offered for `script`.
#[cfg(test)]
pub(crate) fn offers(script: &str, output: &str, fix: &str) -> bool {
    fixes_for(script, output)
        .iter()
        .any(|offered| offered == fix)
}

/// Whether the named rule matches, on its own: no other rule is involved.
#[cfg(test)]
pub(crate) fn rule_matches(name: &str, script: &str, output: &str) -> bool {
    let rule = all()
        .into_iter()
        .find(|rule| rule.name == name)
        .unwrap_or_else(|| panic!("no rule named {name}"));
    (rule.matches)(&Command::new(script, output))
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

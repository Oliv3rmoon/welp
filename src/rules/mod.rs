//! Rules that turn a failed command into fixes. Each one is a port of the
//! thefuck rule of the same name (MIT, https://github.com/nvbn/thefuck),
//! so `exclude_rules` from a thefuck config keeps working.

mod files;
mod git;
mod git_extra;
mod more_git;
mod more_pkg;
mod more_shell;
mod more_tools;
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
    rules.extend(more_git::rules());
    rules.extend(more_shell::rules());
    rules.extend(more_tools::rules());
    rules.extend(more_pkg::rules());
    rules.sort_by_key(|rule| rule.priority);
    rules
}

/// Rules that are off unless named in `enable_rules`, because their fix
/// deletes, overwrites or kills something the typed command didn't ask to.
/// thefuck ships the same ones with `enabled_by_default = False` or worse.
pub fn disabled_by_default() -> Vec<&'static str> {
    [
        more_git::DISABLED,
        more_shell::DISABLED,
        more_tools::DISABLED,
        more_pkg::DISABLED,
    ]
    .concat()
}

/// What a read-only listing command prints, such as `gradle tasks` or
/// `yarn --help`, for rules that need a list of valid names. It runs with a
/// short timeout and `None` means it failed or isn't installed. Never pass
/// it anything that changes state.
pub(crate) fn listing(program: &str, args: &[&str]) -> Option<String> {
    crate::runner::run_listing(program, args)
}

/// A fixed command and the rule that came up with it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fix {
    pub command: String,
    pub rule: String,
}

/// Every fix the enabled rules offer, best first, without repeats.
pub fn suggest(command: &Command, config: &Config) -> Vec<Fix> {
    let mut fixes: Vec<Fix> = Vec::new();
    let disabled = disabled_by_default();
    for rule in all() {
        let off = disabled.contains(&rule.name)
            && !config.enable_rules.iter().any(|name| name == rule.name);
        if off
            || config.exclude_rules.iter().any(|name| name == rule.name)
            || !(rule.matches)(command)
        {
            continue;
        }
        for fix in (rule.fixes)(command) {
            let fix = fix.trim().to_string();
            if !fix.is_empty()
                && fix != command.script
                && !fixes.iter().any(|known| known.command == fix)
            {
                fixes.push(Fix {
                    command: fix,
                    rule: rule.name.to_string(),
                });
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
        .into_iter()
        .map(|fix| fix.command)
        .collect()
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
    fn listing_reads_what_a_program_prints() {
        assert_eq!(
            listing("echo", &["tasks"]).as_deref().map(str::trim),
            Some("tasks")
        );
        assert_eq!(
            listing("welp-no-such-program", &[]),
            None.or(listing("welp-no-such-program", &[]))
        );
    }

    #[test]
    fn risky_rules_need_opting_in() {
        assert!(
            disabled_by_default()
                .iter()
                .all(|name| all().iter().any(|rule| rule.name == *name))
        );
    }

    #[test]
    fn excluded_rules_are_skipped() {
        let config = Config {
            exclude_rules: vec!["sl_ls".into(), "no_command".into()],
            ..Config::default()
        };
        assert!(suggest(&Command::new("sl", ""), &config).is_empty());
    }
}

#[cfg(test)]
mod ordering {
    use super::*;

    #[test]
    fn a_missing_space_beats_a_far_fetched_typo() {
        let missing = all()
            .into_iter()
            .find(|rule| rule.name == "missing_space_before_subcommand")
            .unwrap();
        let typo = all()
            .into_iter()
            .find(|rule| rule.name == "no_command")
            .unwrap();
        assert!(missing.priority < typo.priority);
    }

    #[test]
    fn switching_to_main_beats_creating_master() {
        let main_master = all()
            .into_iter()
            .find(|rule| rule.name == "git_main_master")
            .unwrap();
        let checkout = all()
            .into_iter()
            .find(|rule| rule.name == "git_checkout")
            .unwrap();
        assert!(main_master.priority < checkout.priority);
    }
}

//! More git rules ported from thefuck.
//!
//! `git_push_force` (a force push) and `git_tag_force` (moving a tag) are off
//! by default; thefuck itself ships `git_push_force` turned off.
//!
//! Three rules keep only the safe part of what thefuck offers, the way
//! `git_branch_exists` does:
//!
//! - `git_branch_0flag` clears the stray branch (`0a` for `-a`) with
//!   `branch -d`, which refuses unmerged work, not `branch -D`.
//! - `git_branch_delete_checked_out` keeps the flag you typed. thefuck turns
//!   `-d` into `-D`, which deletes a branch with work nobody merged. It also
//!   moves to `main` when the repository has one and not only to `master`.
//! - `git_rebase_merge_dir` offers `--abort`, `--skip` and `--continue` but
//!   not the `rm -fr .git/rebase-merge` line git prints.
//!
//! `git_pull_uncommitted_changes` keeps the arguments of the pull it stashes
//! around, where thefuck always writes a bare `git pull`.

use std::sync::LazyLock;

use regex::Regex;

use super::git::has_word;
use super::more_shell::{args, close_matches, closest, is_app, matched_commands, replace_command};
use super::{Rule, listing, replace_arg};
use crate::command::Command;

/// Rules in this file that are off unless the user enables them.
pub const DISABLED: &[&str] = &[
    // force-pushes over the commits on the remote
    "git_push_force",
    // moves a tag that already exists
    "git_tag_force",
];

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "git_bisect_usage",
            description: "Uses the git bisect subcommand that looks like the one you typed",
            priority: 1000,
            matches: |c| {
                is_app(c, "git") && has_word(c, "bisect") && c.output.contains("usage: git bisect")
            },
            fixes: git_bisect_usage,
        },
        Rule {
            name: "git_branch_0flag",
            description: "Turns 0a, 0d, 0v and friends into -a, -d, -v after git branch",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && args(c).first().is_some_and(|sub| sub == "branch")
                    && zero_flag(c).is_some()
            },
            fixes: git_branch_0flag,
        },
        Rule {
            name: "git_branch_delete_checked_out",
            description: "Leaves the branch, then deletes it",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && (c.script.contains("branch -d") || c.script.contains("branch -D"))
                    && c.output.contains("error: Cannot delete branch '")
                    && c.output.contains("' checked out at '")
            },
            fixes: |c| {
                let branches = listing("git", &["branch", "--list", "main", "master"]);
                vec![leave_branch_first(c, branches.as_deref())]
            },
        },
        Rule {
            name: "git_branch_list",
            description: "Deletes the branch called list that `git branch list` made, then lists branches",
            priority: 1000,
            matches: |c| c.parts == ["git", "branch", "list"],
            fixes: |_| vec!["git branch --delete list && git branch".into()],
        },
        Rule {
            name: "git_clone_missing",
            description: "Adds git clone in front of a pasted repository address",
            priority: 1000,
            matches: |c| pasted_address(c, is_on_path),
            fixes: |c| vec![format!("git clone {}", c.script)],
        },
        Rule {
            name: "git_commit_amend",
            description: "Amends the commit you just made",
            priority: 1200,
            matches: |c| is_app(c, "git") && has_word(c, "commit"),
            fixes: |_| vec!["git commit --amend".into()],
        },
        Rule {
            name: "git_commit_reset",
            description: "Undoes the last commit and keeps its changes",
            priority: 1200,
            matches: |c| is_app(c, "git") && has_word(c, "commit"),
            fixes: |_| vec!["git reset HEAD~".into()],
        },
        Rule {
            name: "git_diff_no_index",
            description: "Adds --no-index to diff two files git does not track",
            priority: 1100,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "diff")
                    && !c.script.contains("--no-index")
                    && args(c)
                        .iter()
                        .skip(1)
                        .filter(|arg| !arg.starts_with('-'))
                        .count()
                        == 2
            },
            fixes: |c| vec![replace_arg(&c.script, "diff", "diff --no-index")],
        },
        Rule {
            name: "git_diff_staged",
            description: "Adds --staged to diff what is staged",
            priority: 1200,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "diff")
                    && !c.script.contains("--staged")
                    && !c.script.contains("--cached")
            },
            fixes: |c| vec![replace_arg(&c.script, "diff", "diff --staged")],
        },
        Rule {
            name: "git_fix_stash",
            description: "Fixes a misspelled git stash subcommand, or saves with a message",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && args(c).first().is_some_and(|sub| sub == "stash")
                    && c.output.contains("usage:")
            },
            fixes: git_fix_stash,
        },
        Rule {
            name: "git_flag_after_filename",
            description: "Moves a flag in front of the file name",
            priority: 1000,
            matches: |c| is_app(c, "git") && bad_flag(&c.output).is_some(),
            fixes: git_flag_after_filename,
        },
        Rule {
            name: "git_help_aliased",
            description: "Opens help for the command an alias stands for",
            priority: 1000,
            matches: |c| {
                is_app(c, "git") && has_word(c, "help") && c.output.contains(" is aliased to ")
            },
            fixes: |c| {
                c.output
                    .splitn(3, '`')
                    .nth(2)
                    .and_then(|rest| rest.split('\'').next())
                    .and_then(|aliased| aliased.split(' ').next())
                    .map(|aliased| format!("git help {aliased}"))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "git_lfs_mistype",
            description: "Uses the git lfs command git-lfs suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, "git") && has_word(c, "lfs") && c.output.contains("Did you mean this?")
            },
            fixes: git_lfs_mistype,
        },
        Rule {
            name: "git_merge_unrelated",
            description: "Adds --allow-unrelated-histories to a merge git refuses",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "merge")
                    && c.output
                        .contains("fatal: refusing to merge unrelated histories")
            },
            fixes: |c| vec![format!("{} --allow-unrelated-histories", c.script)],
        },
        Rule {
            name: "git_pull_clone",
            description: "Clones the repository you tried to pull outside of any repository",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "pull")
                    && pulled_from(c).is_some()
                    && c.output_lower().contains("fatal: not a git repository")
                    && c.output.contains(
                        "Stopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).",
                    )
            },
            fixes: |c| vec![replace_arg(&c.script, "pull", "clone")],
        },
        Rule {
            name: "git_pull_uncommitted_changes",
            description: "Stashes your changes around the pull",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "pull")
                    && (c.output.contains("You have unstaged changes")
                        || c.output.contains("contains uncommitted changes"))
            },
            fixes: |c| vec![format!("git stash && {} && git stash pop", c.script)],
        },
        Rule {
            name: "git_push_different_branch_names",
            description: "Pushes the way git says to when the branch names differ",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "push")
                    && c.output
                        .contains("The upstream branch of your current branch does not match")
            },
            fixes: |c| {
                static PUSH_HINT: LazyLock<Regex> =
                    LazyLock::new(|| Regex::new(r"(?m)^ +(git push \S+ \S+)").unwrap());
                PUSH_HINT
                    .captures(&c.output)
                    .map(|hint| hint[1].to_string())
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "git_push_force",
            description: "Pushes with --force-with-lease when the remote is ahead",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "push")
                    && c.output.contains("! [rejected]")
                    && c.output.contains("failed to push some refs to")
                    && c.output.contains(
                        "Updates were rejected because the tip of your current branch is behind",
                    )
            },
            fixes: |c| vec![replace_arg(&c.script, "push", "push --force-with-lease")],
        },
        Rule {
            name: "git_push_without_commits",
            description: "Makes the first commit, then pushes",
            priority: 1000,
            matches: |c| is_app(c, "git") && NO_COMMITS.is_match(&c.output),
            fixes: |c| vec![format!("git commit -m \"Initial commit\" && {}", c.script)],
        },
        Rule {
            name: "git_rebase_merge_dir",
            description: "Continues, skips or aborts the rebase that is already going",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "rebase")
                    && c.output
                        .contains("It seems that there is already a rebase-merge directory")
                    && c.output
                        .contains("I wonder if you are in the middle of another rebase")
            },
            fixes: |c| {
                close_matches(
                    &c.script,
                    &[
                        "git rebase --continue",
                        "git rebase --abort",
                        "git rebase --skip",
                    ],
                    4,
                    0.0,
                )
            },
        },
        Rule {
            name: "git_remote_delete",
            description: "git remote remove, not delete",
            priority: 1000,
            matches: |c| is_app(c, "git") && c.script.contains("remote delete"),
            fixes: |c| vec![c.script.replacen("remote delete", "remote remove", 1)],
        },
        Rule {
            name: "git_stash_pop",
            description: "Stages the changes that block git stash pop, pops, then unstages",
            // before the other rules that match the same error
            priority: 900,
            matches: |c| {
                is_app(c, "git")
                    && has_word(c, "stash")
                    && has_word(c, "pop")
                    && c.output.contains(
                        "Your local changes to the following files would be overwritten by merge",
                    )
            },
            fixes: |_| vec!["git add --update && git stash pop && git reset .".into()],
        },
        Rule {
            name: "git_tag_force",
            description: "Adds --force to move a tag that already exists",
            priority: 1000,
            matches: |c| {
                is_app(c, "git") && has_word(c, "tag") && c.output.contains("already exists")
            },
            fixes: |c| vec![replace_arg(&c.script, "tag", "tag --force")],
        },
        Rule {
            name: "git_two_dashes",
            description: "Puts the second dash on a long option, as git asks",
            priority: 1000,
            matches: |c| {
                is_app(c, "git")
                    && c.output.contains("error: did you mean `")
                    && c.output.contains("` (with two dashes ?)")
            },
            fixes: |c| {
                c.output
                    .split('`')
                    .nth(1)
                    .and_then(|long| {
                        long.strip_prefix('-')
                            .map(|short| replace_arg(&c.script, short, long))
                    })
                    .into_iter()
                    .collect()
            },
        },
    ]
}

/// The word after `word` in the command.
fn after<'a>(c: &'a Command, word: &str) -> Option<&'a String> {
    let at = c.parts.iter().position(|part| part == word)?;
    c.parts.get(at + 1)
}

fn git_bisect_usage(c: &Command) -> Vec<String> {
    static USAGE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"usage: git bisect \[([^\]]+)\]").unwrap());
    let (Some(broken), Some(usage)) = (after(c, "bisect"), USAGE.captures(&c.output)) else {
        return Vec::new();
    };
    let subcommands: Vec<&str> = usage[1].split('|').collect();
    replace_command(&c.script, broken, &subcommands)
}

/// The first `0x` word, which is `-x` typed with a zero for the dash.
fn zero_flag(c: &Command) -> Option<&String> {
    args(c)
        .iter()
        .find(|part| part.len() == 2 && part.starts_with('0'))
}

fn git_branch_0flag(c: &Command) -> Vec<String> {
    let Some(zero) = zero_flag(c) else {
        return Vec::new();
    };
    let fixed = replace_arg(&c.script, zero, &zero.replace('0', "-"));
    if c.output.contains("A branch named '") && c.output.contains("' already exists.") {
        // `-d`, so a branch with work that nothing merged is left alone
        return vec![format!(
            "git branch -d {} && {fixed}",
            crate::command::quote(zero)
        )];
    }
    vec![fixed]
}

/// The command, after a checkout of the main branch to get off the one it
/// deletes. `branches` is what `git branch --list main master` printed.
fn leave_branch_first(c: &Command, branches: Option<&str>) -> String {
    format!("git checkout {} && {}", default_branch(branches), c.script)
}

/// `main` when the repository has a branch of that name, otherwise `master`.
fn default_branch(branches: Option<&str>) -> &'static str {
    let has_main = branches.is_some_and(|branches| {
        branches
            .lines()
            .any(|line| line.trim_start_matches(['*', ' ']).trim() == "main")
    });
    if has_main { "main" } else { "master" }
}

/// Whether the whole command is a repository address pasted without
/// `git clone` in front of it, and the shell did not know it as a program.
fn pasted_address(c: &Command, is_command: impl Fn(&str) -> bool) -> bool {
    let [address] = c.parts.as_slice() else {
        return false;
    };
    if is_command(address) {
        return false;
    }
    let output = &c.output;
    if !(output.contains("No such file or directory")
        || output.contains("not found")
        || output.contains("is not recognised as"))
    {
        return false;
    }
    if let Some(rest) = address
        .strip_prefix("https://")
        .or_else(|| address.strip_prefix("http://"))
    {
        // a web address needs a host
        return !rest.is_empty() && !rest.starts_with('/');
    }
    // ssh://git@host:path, or git@host:path with no scheme at all
    (address.starts_with("ssh://") || !address.contains("://"))
        && address.contains('@')
        && address.contains(':')
}

fn is_on_path(name: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(name).is_file()))
}

fn git_fix_stash(c: &Command) -> Vec<String> {
    const STASH_COMMANDS: [&str; 8] = [
        "apply", "branch", "clear", "drop", "list", "pop", "save", "show",
    ];
    let Some(sub) = args(c).get(1) else {
        return Vec::new();
    };
    match closest(sub, &STASH_COMMANDS, 0.6, false) {
        Some(fixed) => vec![replace_arg(&c.script, sub, &fixed)],
        None => vec![c.script.replacen(" stash ", " stash save ", 1)],
    }
}

/// The flag git says came after a file name.
fn bad_flag(output: &str) -> Option<String> {
    static AFTER_FILENAME: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"fatal: bad flag '(.*?)' used after filename").unwrap());
    static BEFORE_NON_OPTIONS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"fatal: option '(.*?)' must come before non-option arguments").unwrap()
    });
    AFTER_FILENAME
        .captures(output)
        .or_else(|| BEFORE_NON_OPTIONS.captures(output))
        .map(|found| found[1].to_string())
}

fn git_flag_after_filename(c: &Command) -> Vec<String> {
    let Some(flag) = bad_flag(&c.output) else {
        return Vec::new();
    };
    let Some(flag_at) = c.parts.iter().position(|part| *part == flag) else {
        return Vec::new();
    };
    // the closest word before the flag that is not a flag itself (and not
    // `git` or the subcommand)
    let Some(file_at) = (2..flag_at).rev().find(|at| !c.parts[*at].starts_with('-')) else {
        return Vec::new();
    };
    let mut parts = c.parts.clone();
    parts.swap(flag_at, file_at);
    vec![shell_words::join(parts)]
}

fn git_lfs_mistype(c: &Command) -> Vec<String> {
    static UNKNOWN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"Error: unknown command "([^"]*)" for "git-lfs""#).unwrap());
    let Some(broken) = UNKNOWN.captures(&c.output) else {
        return Vec::new();
    };
    let matched = matched_commands(&c.output, &["Did you mean", " for usage."]);
    replace_command(&c.script, &broken[1], &matched)
}

/// The word after `pull` that is not a flag: the repository to pull from.
fn pulled_from(c: &Command) -> Option<&String> {
    let at = c.parts.iter().position(|part| part == "pull")?;
    c.parts[at + 1..].iter().find(|part| !part.starts_with('-'))
}

static NO_COMMITS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"src refspec \S+ does not match any").unwrap());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{all, rule_matches};

    /// The fixes one rule gives for a command, whether or not other rules or
    /// its being off by default would get in the way.
    fn fixes(name: &str, script: &str, output: &str) -> Vec<String> {
        let rule = all()
            .into_iter()
            .find(|rule| rule.name == name)
            .unwrap_or_else(|| panic!("no rule named {name}"));
        let command = Command::new(script, output);
        if (rule.matches)(&command) {
            (rule.fixes)(&command)
        } else {
            Vec::new()
        }
    }

    const BISECT_USAGE: &str = "usage: git bisect [help|start|bad|good|new|old|terms|skip|next|reset|visualize|replay|log|run]";

    #[test]
    fn bisect_offers_the_subcommands_that_look_alike() {
        for (script, fixed) in [
            ("git bisect goood", ["good", "old", "log"]),
            ("git bisect strt", ["start", "terms", "reset"]),
            ("git bisect rset", ["reset", "next", "start"]),
        ] {
            let want: Vec<String> = fixed
                .iter()
                .map(|sub| format!("git bisect {sub}"))
                .collect();
            assert_eq!(fixes("git_bisect_usage", script, BISECT_USAGE), want);
        }
        for script in ["git bisect", "git bisect start", "git bisect good"] {
            assert!(!rule_matches("git_bisect_usage", script, ""), "{script}");
        }
    }

    #[test]
    fn branch_0flag_turns_the_zero_into_a_dash() {
        let exists = "fatal: A branch named 'bar' already exists.";
        for script in [
            "git branch 0a",
            "git branch 0d",
            "git branch 0f",
            "git branch 0r",
            "git branch 0v",
            "git branch 0d foo",
            "git branch 0D foo",
        ] {
            assert!(rule_matches("git_branch_0flag", script, exists), "{script}");
        }
        for (script, fixed) in [
            ("git branch 0a", "git branch -d 0a && git branch -a"),
            ("git branch 0v", "git branch -d 0v && git branch -v"),
            ("git branch 0d foo", "git branch -d 0d && git branch -d foo"),
            ("git branch 0D foo", "git branch -d 0D && git branch -D foo"),
            (
                "git branch 0l 'maint-*'",
                "git branch -d 0l && git branch -l 'maint-*'",
            ),
            (
                "git branch 0u upstream",
                "git branch -d 0u && git branch -u upstream",
            ),
        ] {
            assert_eq!(fixes("git_branch_0flag", script, exists), [fixed]);
        }
        // the branch is not there yet
        for (script, fixed) in [
            ("git branch 0l 'maint-*'", "git branch -l 'maint-*'"),
            ("git branch 0u upstream", "git branch -u upstream"),
        ] {
            assert_eq!(
                fixes(
                    "git_branch_0flag",
                    script,
                    "fatal: Not a valid object name: 'bar'."
                ),
                [fixed]
            );
        }
        for script in [
            "git branch -a",
            "git branch -r",
            "git branch -v",
            "git branch -d foo",
            "git branch -D foo",
            "git branch 0",
            "git checkout 0a",
        ] {
            assert!(
                !rule_matches("git_branch_0flag", script, exists),
                "{script}"
            );
        }
    }

    #[test]
    fn deleting_a_checked_out_branch_leaves_it_first() {
        let output = "error: Cannot delete branch 'foo' checked out at '/bar/foo'";
        // keeps the flag it was given: -d stays -d
        for script in ["git branch -d foo", "git branch -D foo"] {
            assert!(rule_matches(
                "git_branch_delete_checked_out",
                script,
                output
            ));
            assert!(!rule_matches(
                "git_branch_delete_checked_out",
                script,
                "Deleted branch foo (was a1b2c3d)."
            ));
            let command = Command::new(script, output);
            assert_eq!(
                leave_branch_first(&command, None),
                format!("git checkout master && {script}")
            );
            assert_eq!(
                leave_branch_first(&command, Some("  main\n")),
                format!("git checkout main && {script}")
            );
        }
        assert_eq!(default_branch(Some("  main\n* master\n")), "main");
        assert_eq!(default_branch(Some("* master\n")), "master");
        assert_eq!(default_branch(Some("  maintenance\n")), "master");
        assert_eq!(default_branch(None), "master");
        assert!(!rule_matches(
            "git_branch_delete_checked_out",
            "git branch foo",
            output
        ));
    }

    #[test]
    fn branch_list_deletes_the_branch_it_made() {
        assert_eq!(
            fixes("git_branch_list", "git branch list", ""),
            ["git branch --delete list && git branch"]
        );
        for script in [
            "",
            "git commit",
            "git branch",
            "git stash list",
            "git branch list x",
        ] {
            assert!(!rule_matches("git_branch_list", script, ""), "{script}");
        }
    }

    #[test]
    fn clone_missing_adds_git_clone() {
        let nowhere = |_: &str| false;
        let valid = [
            "https://github.com/nvbn/thefuck.git",
            "https://github.com/nvbn/thefuck",
            "http://github.com/nvbn/thefuck.git",
            "git@github.com:nvbn/thefuck.git",
            "git@github.com:nvbn/thefuck",
            "ssh://git@github.com:nvbn/thefuck.git",
        ];
        let invalid = [
            "",
            "notacommand",
            "ssh git@github.com:nvbn/thefrick.git",
            "git clone foo",
            "git clone https://github.com/nvbn/thefuck.git",
            "github.com/nvbn/thefuck.git",
            "github.com:nvbn/thefuck.git",
            "git clone git clone ssh://git@github.com:nvbn/thefrick.git",
            "https:/github.com/nvbn/thefuck.git",
            "ssh://github.com/nvbn/thefuck.git",
        ];
        let outputs = [
            "No such file or directory",
            "not found",
            "is not recognised as",
        ];
        for address in valid {
            for output in outputs {
                assert!(
                    pasted_address(&Command::new(address, output), nowhere),
                    "{address}"
                );
            }
            // it is the name of a program after all, or nothing went wrong
            assert!(
                !pasted_address(&Command::new(address, outputs[0]), |_| true),
                "{address}"
            );
            assert!(
                !pasted_address(&Command::new(address, "some other output"), nowhere),
                "{address}"
            );
            assert_eq!(
                fixes("git_clone_missing", address, outputs[1]),
                [format!("git clone {address}")]
            );
        }
        for address in invalid {
            for output in outputs.iter().copied().chain(["some other output"]) {
                assert!(
                    !pasted_address(&Command::new(address, output), nowhere),
                    "{address}"
                );
            }
        }
    }

    #[test]
    fn commit_can_be_amended_or_undone() {
        for (script, output) in [
            ("git commit -m \"test\"", "test output"),
            ("git commit", ""),
        ] {
            assert_eq!(
                fixes("git_commit_amend", script, output),
                ["git commit --amend"]
            );
            assert_eq!(
                fixes("git_commit_reset", script, output),
                ["git reset HEAD~"]
            );
        }
        for script in [
            "git branch foo",
            "git checkout feature/test_commit",
            "git push",
        ] {
            assert!(!rule_matches("git_commit_amend", script, ""), "{script}");
            assert!(!rule_matches("git_commit_reset", script, ""), "{script}");
        }
    }

    #[test]
    fn diff_of_two_untracked_files() {
        assert_eq!(
            fixes("git_diff_no_index", "git diff foo bar", ""),
            ["git diff --no-index foo bar"]
        );
        for script in [
            "git diff --no-index foo bar",
            "git diff foo",
            "git diff foo bar baz",
            "git log foo bar",
        ] {
            assert!(!rule_matches("git_diff_no_index", script, ""), "{script}");
        }
    }

    #[test]
    fn diff_of_what_is_staged() {
        assert_eq!(
            fixes("git_diff_staged", "git diff", ""),
            ["git diff --staged"]
        );
        assert_eq!(
            fixes("git_diff_staged", "git diff foo", ""),
            ["git diff --staged foo"]
        );
        for script in [
            "git diff --staged",
            "git diff --cached",
            "git tag",
            "git branch",
            "git log",
        ] {
            assert!(!rule_matches("git_diff_staged", script, ""), "{script}");
        }
    }

    const STASH_USAGE: &str = "\nusage: git stash list [<options>]\n   or: git stash show [<stash>]\n   or: git stash drop [-q|--quiet] [<stash>]\n   or: git stash ( pop | apply ) [--index] [-q|--quiet] [<stash>]\n   or: git stash branch <branchname> [<stash>]\n   or: git stash [save [--patch] [-k|--[no-]keep-index] [-q|--quiet]\n\t\t       [-u|--include-untracked] [-a|--all] [<message>]]\n   or: git stash clear\n";

    #[test]
    fn stash_with_a_typo_or_a_message() {
        for (script, fixed) in [
            ("git stash opp", "git stash pop"),
            ("git stash Some message", "git stash save Some message"),
            ("git stash saev Some message", "git stash save Some message"),
        ] {
            assert_eq!(fixes("git_fix_stash", script, STASH_USAGE), [fixed]);
        }
        assert!(!rule_matches("git_fix_stash", "git", STASH_USAGE));
        assert!(!rule_matches("git_fix_stash", "git stash pop", ""));
        assert!(!rule_matches("git_fix_stash", "git status", STASH_USAGE));
    }

    #[test]
    fn flag_goes_before_the_file() {
        for template in [
            "fatal: bad flag 'FLAG' used after filename",
            "fatal: option 'FLAG' must come before non-option arguments",
        ] {
            for (script, flag, fixed) in [
                ("git log README.md -p", "-p", "git log -p README.md"),
                (
                    "git log README.md -p CONTRIBUTING.md",
                    "-p",
                    "git log -p README.md CONTRIBUTING.md",
                ),
                (
                    "git log -p README.md --name-only",
                    "--name-only",
                    "git log -p --name-only README.md",
                ),
            ] {
                let output = template.replace("FLAG", flag);
                assert_eq!(fixes("git_flag_after_filename", script, &output), [fixed]);
            }
        }
        for script in ["git log README.md", "git log -p README.md"] {
            assert!(
                !rule_matches("git_flag_after_filename", script, ""),
                "{script}"
            );
        }
        // a flag that is not in the command, or no file in front of it
        assert!(
            fixes(
                "git_flag_after_filename",
                "git log",
                "fatal: bad flag '-p' used after filename"
            )
            .is_empty()
        );
        assert!(
            fixes(
                "git_flag_after_filename",
                "git -p",
                "fatal: bad flag '-p' used after filename"
            )
            .is_empty()
        );
    }

    #[test]
    fn help_for_what_the_alias_stands_for() {
        for (script, output, fixed) in [
            (
                "git help st",
                "`git st' is aliased to `status'",
                "git help status",
            ),
            (
                "git help ds",
                "`git ds' is aliased to `diff --staged'",
                "git help diff",
            ),
        ] {
            assert_eq!(fixes("git_help_aliased", script, output), [fixed]);
        }
        for (script, output) in [
            (
                "git help status",
                "GIT-STATUS(1)...Git Manual...GIT-STATUS(1)",
            ),
            ("git help diff", "GIT-DIFF(1)...Git Manual...GIT-DIFF(1)"),
        ] {
            assert!(
                !rule_matches("git_help_aliased", script, output),
                "{script}"
            );
        }
    }

    #[test]
    fn lfs_command_git_lfs_suggests() {
        let mistype = "\nError: unknown command \"evn\" for \"git-lfs\"\nDid you mean this?\n        env\n        ext\nRun 'git-lfs --help' for usage.\n    ";
        assert_eq!(
            fixes("git_lfs_mistype", "git lfs evn", mistype),
            ["git lfs env", "git lfs ext"]
        );
        assert!(!rule_matches(
            "git_lfs_mistype",
            "git lfs env",
            "bash: git: command not found"
        ));
        assert!(!rule_matches("git_lfs_mistype", "docker lfs env", mistype));
    }

    #[test]
    fn merge_allows_unrelated_histories() {
        let output = "fatal: refusing to merge unrelated histories";
        for (script, fixed) in [
            (
                "git merge local",
                "git merge local --allow-unrelated-histories",
            ),
            (
                "git merge -m \"test\" local",
                "git merge -m \"test\" local --allow-unrelated-histories",
            ),
            (
                "git merge -m \"test local\" local",
                "git merge -m \"test local\" local --allow-unrelated-histories",
            ),
        ] {
            assert_eq!(fixes("git_merge_unrelated", script, output), [fixed]);
        }
        assert!(!rule_matches("git_merge_unrelated", "git merge master", ""));
        assert!(!rule_matches("git_merge_unrelated", "ls", output));
    }

    #[test]
    fn pull_becomes_clone_outside_a_repository() {
        let error = "\nfatal: Not a git repository (or any parent up to mount point /home)\nStopping at filesystem boundary (GIT_DISCOVERY_ACROSS_FILESYSTEM not set).\n";
        assert_eq!(
            fixes(
                "git_pull_clone",
                "git pull git@github.com:mcarton/thefuck.git",
                error
            ),
            ["git clone git@github.com:mcarton/thefuck.git"]
        );
        // nothing to clone from, or an ordinary error
        assert!(!rule_matches("git_pull_clone", "git pull", error));
        assert!(!rule_matches("git_pull_clone", "git pull --rebase", error));
        assert!(!rule_matches(
            "git_pull_clone",
            "git pull origin",
            "fatal: not a git repository (or any of the parent directories): .git"
        ));
    }

    #[test]
    fn pull_with_changes_stashes_around_it() {
        let output = "error: Cannot pull with rebase: You have unstaged changes.";
        assert_eq!(
            fixes("git_pull_uncommitted_changes", "git pull", output),
            ["git stash && git pull && git stash pop"]
        );
        assert_eq!(
            fixes(
                "git_pull_uncommitted_changes",
                "git pull --rebase origin main",
                output
            ),
            ["git stash && git pull --rebase origin main && git stash pop"]
        );
        assert!(rule_matches(
            "git_pull_uncommitted_changes",
            "git pull",
            "error: Your local changes would be lost: the index contains uncommitted changes."
        ));
        assert!(!rule_matches(
            "git_pull_uncommitted_changes",
            "git pull",
            ""
        ));
        assert!(!rule_matches("git_pull_uncommitted_changes", "ls", output));
    }

    #[test]
    fn push_the_way_git_says_for_different_names() {
        let output = |local: &str, remote: &str| {
            format!(
                "fatal: The upstream branch of your current branch does not match\nthe name of your current branch.  To push to the upstream branch\non the remote, use\n    git push origin HEAD:{remote}\nTo push to the branch of the same name on the remote, use\n    git push origin {local}\nTo choose either option permanently, see push.default in 'git help config'.\n"
            )
        };
        assert_eq!(
            fixes(
                "git_push_different_branch_names",
                "git push",
                &output("foo", "bar")
            ),
            ["git push origin HEAD:bar"]
        );
        for (script, output) in [
            ("vim", String::new()),
            ("git status", output("foo", "bar")),
            ("git push", String::new()),
        ] {
            assert!(
                !rule_matches("git_push_different_branch_names", script, &output),
                "{script}"
            );
        }
    }

    const PUSH_BEHIND: &str = "\nTo /tmp/foo\n ! [rejected]        master -> master (non-fast-forward)\n error: failed to push some refs to '/tmp/bar'\n hint: Updates were rejected because the tip of your current branch is behind\n hint: its remote counterpart. Integrate the remote changes (e.g.\n hint: 'git pull ...') before pushing again.\n hint: See the 'Note about fast-forwards' in 'git push --help' for details.\n";

    #[test]
    fn force_push_is_off_unless_asked_for() {
        for (script, fixed) in [
            ("git push", "git push --force-with-lease"),
            ("git push nvbn", "git push --force-with-lease nvbn"),
            (
                "git push nvbn master",
                "git push --force-with-lease nvbn master",
            ),
        ] {
            assert_eq!(fixes("git_push_force", script, PUSH_BEHIND), [fixed]);
        }
        for output in [
            "\nCounting objects: 3, done.\nTotal 3 (delta 0), reused 0 (delta 0)\nTo /tmp/bar\n   514eed3..f269c79  master -> master\n",
            "Everything up-to-date",
        ] {
            assert!(!rule_matches("git_push_force", "git push", output));
        }
        assert!(DISABLED.contains(&"git_push_force"));
        assert!(
            !crate::rules::fixes_for("git push", PUSH_BEHIND)
                .iter()
                .any(|fix| fix.contains("--force"))
        );
    }

    #[test]
    fn push_without_commits_commits_first() {
        let output = "error: src refspec master does not match any\nerror: failed to...";
        assert_eq!(
            fixes(
                "git_push_without_commits",
                "git push -u origin master",
                output
            ),
            ["git commit -m \"Initial commit\" && git push -u origin master"]
        );
        assert_eq!(
            fixes(
                "git_push_without_commits",
                "git push -u origin feature/x",
                "error: src refspec feature/x does not match any"
            ),
            ["git commit -m \"Initial commit\" && git push -u origin feature/x"]
        );
        assert!(!rule_matches(
            "git_push_without_commits",
            "git push -u origin master",
            "Everything up-to-date"
        ));
    }

    #[test]
    fn rebase_already_going_offers_the_safe_ways_out() {
        let output = "\n\nIt seems that there is already a rebase-merge directory, and\nI wonder if you are in the middle of another rebase.  If that is the\ncase, please try\n\tgit rebase (--continue | --abort | --skip)\nIf that is not the case, please\n\trm -fr \"/foo/bar/baz/egg/.git/rebase-merge\"\nand run me again.  I am stopping in case you still have something\nvaluable there.\n";
        for (script, fixed) in [
            (
                "git rebase master",
                [
                    "git rebase --abort",
                    "git rebase --skip",
                    "git rebase --continue",
                ],
            ),
            (
                "git rebase -skip",
                [
                    "git rebase --skip",
                    "git rebase --abort",
                    "git rebase --continue",
                ],
            ),
            (
                "git rebase",
                [
                    "git rebase --skip",
                    "git rebase --abort",
                    "git rebase --continue",
                ],
            ),
        ] {
            assert_eq!(fixes("git_rebase_merge_dir", script, output), fixed);
        }
        // git's own rm -fr line is not offered
        assert!(
            !fixes("git_rebase_merge_dir", "git rebase master", output)
                .iter()
                .any(|fix| fix.contains("rm "))
        );
        for script in ["git rebase master", "git rebase -abort"] {
            assert!(
                !rule_matches("git_rebase_merge_dir", script, ""),
                "{script}"
            );
        }
    }

    #[test]
    fn remote_remove_not_delete() {
        assert_eq!(
            fixes("git_remote_delete", "git remote delete foo", ""),
            ["git remote remove foo"]
        );
        assert_eq!(
            fixes("git_remote_delete", "git remote delete delete", ""),
            ["git remote remove delete"]
        );
        for script in ["git remote remove foo", "git remote add foo", "git commit"] {
            assert!(!rule_matches("git_remote_delete", script, ""), "{script}");
        }
    }

    #[test]
    fn stash_pop_stages_what_is_in_the_way() {
        let output =
            "error: Your local changes to the following files would be overwritten by merge:";
        assert_eq!(
            fixes("git_stash_pop", "git stash pop", output),
            ["git add --update && git stash pop && git reset ."]
        );
        assert!(!rule_matches("git_stash_pop", "git stash", ""));
        assert!(!rule_matches("git_stash_pop", "git stash pop", ""));
        assert!(!rule_matches("git_stash_pop", "git stash", output));
    }

    #[test]
    fn tag_force_is_off_unless_asked_for() {
        let output = "fatal: tag 'alert' already exists";
        assert_eq!(
            fixes("git_tag_force", "git tag alert", output),
            ["git tag --force alert"]
        );
        assert!(!rule_matches("git_tag_force", "git tag alert", ""));
        assert!(DISABLED.contains(&"git_tag_force"));
        assert!(
            !crate::rules::fixes_for("git tag alert", output)
                .iter()
                .any(|fix| fix.contains("--force"))
        );
    }

    #[test]
    fn long_options_get_their_second_dash() {
        let error = |long: &str| format!("error: did you mean `{long}` (with two dashes ?)");
        for (script, long, fixed) in [
            ("git add -patch", "--patch", "git add --patch"),
            ("git checkout -patch", "--patch", "git checkout --patch"),
            ("git init -bare", "--bare", "git init --bare"),
            ("git commit -amend", "--amend", "git commit --amend"),
            ("git push -tags", "--tags", "git push --tags"),
            (
                "git rebase -continue",
                "--continue",
                "git rebase --continue",
            ),
        ] {
            assert_eq!(fixes("git_two_dashes", script, &error(long)), [fixed]);
        }
        for script in [
            "git add --patch",
            "git checkout --patch",
            "git commit --amend",
            "git push --tags",
            "git rebase --continue",
        ] {
            assert!(!rule_matches("git_two_dashes", script, ""), "{script}");
        }
    }
}

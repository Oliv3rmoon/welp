//! More git rules, ported from thefuck.
//!
//! Left out on purpose: `git_push_force` and `git_tag_force`. Both overwrite
//! something (remote history, an existing tag) that the command the user
//! typed never asked to overwrite, and thefuck ships `git_push_force` turned
//! off for the same reason.
//!
//! Two rules keep only the safe half of thefuck's fixes. `git_rm_staged` and
//! `git_rm_local_modifications` offer `--cached` but not `-f`, which throws
//! away the changes git is protecting. `git_branch_exists` offers `branch -d`
//! (which refuses unmerged work) but not `branch -D`.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::git::has_word;
use super::{Rule, replace_arg};
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "git_add",
            description: "Stages a file git does not know yet, then runs the command again",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && c.output.contains("did not match any file(s) known to git")
                    && missing_file(c).is_some()
            },
            fixes: |c| {
                missing_file(c)
                    .map(|file| format!("git add -- {} && {}", shell_words::quote(&file), c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "git_add_force",
            description: "Adds --force to stage a file your .gitignore covers",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "add")
                    && c.output.contains("Use -f if you really want to add them.")
            },
            fixes: |c| vec![replace_arg(&c.script, "add", "add --force")],
        },
        Rule {
            name: "git_branch_exists",
            description: "Recreates or switches to a branch that already exists",
            priority: 1000,
            matches: |c| c.is("git") && existing_branch(c).is_some(),
            fixes: git_branch_exists,
        },
        Rule {
            name: "git_clone_git_clone",
            description: "Drops the second `git clone` from a pasted clone command",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && c.script.contains(" git clone ")
                    && c.output.contains("fatal: Too many arguments.")
            },
            fixes: |c| vec![c.script.replacen(" git clone ", " ", 1)],
        },
        Rule {
            name: "git_hook_bypass",
            description: "Adds --no-verify to skip git hooks on am, commit or push",
            priority: 1500,
            matches: |c| c.is("git") && !has_word(c, "--no-verify") && hooked_command(c).is_some(),
            fixes: |c| {
                hooked_command(c)
                    .map(|hooked| replace_arg(&c.script, hooked, &format!("{hooked} --no-verify")))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "git_main_master",
            description: "Swaps main and master when the branch is not there",
            priority: 1200,
            matches: |c| {
                c.is("git") && (c.output.contains("'master'") || c.output.contains("'main'"))
            },
            fixes: |c| {
                let (from, to) = if c.output.contains("'master'") {
                    ("master", "main")
                } else {
                    ("main", "master")
                };
                vec![swap_branch(&c.script, from, to)]
            },
        },
        Rule {
            name: "git_merge",
            description: "Merges the remote branch git suggests",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "merge")
                    && c.output.contains(" - not something we can merge")
                    && c.output.contains("Did you mean this?")
            },
            fixes: git_merge,
        },
        Rule {
            name: "git_push_pull",
            description: "Pulls first when a push is rejected, then pushes",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "push")
                    && c.output.contains("! [rejected]")
                    && c.output.contains("failed to push some refs to")
                    && (c.output.contains(
                        "Updates were rejected because the tip of your current branch is behind",
                    ) || c.output.contains(
                        "Updates were rejected because the remote contains work that you do",
                    ))
            },
            fixes: |c| {
                vec![format!(
                    "{} && {}",
                    replace_arg(&c.script, "push", "pull"),
                    c.script
                )]
            },
        },
        Rule {
            name: "git_rebase_no_changes",
            description: "Skips the patch a rebase finds empty",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "rebase")
                    && has_word(c, "--continue")
                    && c.output
                        .contains("No changes - did you forget to use 'git add'?")
            },
            fixes: |_| vec!["git rebase --skip".into()],
        },
        Rule {
            name: "git_remote_seturl_add",
            description: "Adds the remote when set-url finds none by that name",
            priority: 1000,
            matches: |c| {
                c.is("git") && has_word(c, "set-url") && c.output_lower().contains("no such remote")
            },
            fixes: |c| vec![replace_arg(&c.script, "set-url", "add")],
        },
        Rule {
            name: "git_rm_local_modifications",
            description: "Adds --cached to keep a file with local changes",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "rm")
                    && c.output.contains("local modifications")
                    && c.output
                        .contains("use --cached to keep the file, or -f to force removal")
            },
            fixes: |c| vec![replace_arg(&c.script, "rm", "rm --cached")],
        },
        Rule {
            name: "git_rm_recursive",
            description: "Adds -r to remove a folder",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "rm")
                    && c.output.contains("fatal: not removing '")
                    && c.output.contains("' recursively without -r")
            },
            fixes: |c| vec![replace_arg(&c.script, "rm", "rm -r")],
        },
        Rule {
            name: "git_rm_staged",
            description: "Adds --cached to keep a file with staged changes",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "rm")
                    && c.output.contains("changes staged in the index")
                    && c.output
                        .contains("use --cached to keep the file, or -f to force removal")
            },
            fixes: |c| vec![replace_arg(&c.script, "rm", "rm --cached")],
        },
        Rule {
            name: "git_stash",
            description: "Stashes your changes, then runs the command again",
            priority: 1000,
            matches: |c| c.is("git") && !has_word(c, "stash") && c.output.contains("or stash them"),
            fixes: |c| vec![format!("git stash && {}", c.script)],
        },
    ]
}

static PATHSPEC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"pathspec '([^']*)' did not match any file\(s\) known to git").unwrap()
});

/// The file git says it does not know, when it is on disk (so `git add`
/// can fix it).
fn missing_file(command: &Command) -> Option<String> {
    let found = PATHSPEC.captures(&command.output)?;
    let file = &found[1];
    (!file.is_empty() && Path::new(file).exists()).then(|| file.to_string())
}

static EXISTING_BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)fatal: a branch named '(.+)' already exists").unwrap());

fn existing_branch(command: &Command) -> Option<String> {
    let found = EXISTING_BRANCH.captures(&command.output)?;
    Some(found[1].to_string())
}

/// Recreate the branch where you are, or just switch to it. `branch -d`
/// refuses to delete a branch with unmerged work, so none of these lose any.
fn git_branch_exists(command: &Command) -> Vec<String> {
    let Some(branch) = existing_branch(command) else {
        return Vec::new();
    };
    let branch = shell_words::quote(&branch);
    vec![
        format!("git branch -d {branch} && git branch {branch}"),
        format!("git branch -d {branch} && git checkout -b {branch}"),
        format!("git checkout {branch}"),
    ]
}

/// The first of git's hook-running subcommands the command uses.
fn hooked_command(command: &Command) -> Option<&'static str> {
    ["am", "commit", "push"]
        .into_iter()
        .find(|hooked| has_word(command, hooked))
}

/// Swaps a whole branch name, alone or after a remote (`origin/master`),
/// and leaves longer names such as `master-fix` alone.
fn swap_branch(script: &str, from: &str, to: &str) -> String {
    script
        .split(' ')
        .map(|word| {
            if word == from {
                to.to_string()
            } else if let Some(remote) = word.strip_suffix(&format!("/{from}")) {
                format!("{remote}/{to}")
            } else {
                word.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

static UNKNOWN_BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"merge: (.+) - not something we can merge").unwrap());
static SUGGESTED_BRANCH: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Did you mean this\?\n\t([^\n]+)").unwrap());

fn git_merge(command: &Command) -> Vec<String> {
    let (Some(unknown), Some(suggested)) = (
        UNKNOWN_BRANCH.captures(&command.output),
        SUGGESTED_BRANCH.captures(&command.output),
    ) else {
        return Vec::new();
    };
    vec![replace_arg(
        &command.script,
        &unknown[1],
        suggested[1].trim(),
    )]
}

#[cfg(test)]
mod tests {
    use crate::rules::{fixes_for, offers, rule_matches};

    const PUSH_BEHIND: &str = "\nTo /tmp/foo\n ! [rejected]        master -> master (non-fast-forward)\n error: failed to push some refs to '/tmp/bar'\n hint: Updates were rejected because the tip of your current branch is behind\n hint: its remote counterpart. Integrate the remote changes (e.g.\n hint: 'git pull ...') before pushing again.\n hint: See the 'Note about fast-forwards' in 'git push --help' for details.\n";
    const PUSH_REMOTE_WORK: &str = "\nTo /tmp/foo\n ! [rejected]        master -> master (non-fast-forward)\n error: failed to push some refs to '/tmp/bar'\nhint: Updates were rejected because the remote contains work that you do\nhint: not have locally. This is usually caused by another repository pushing\nhint: to the same ref. You may want to first integrate the remote changes\nhint: (e.g., 'git pull ...') before pushing again.\nhint: See the 'Note about fast-forwards' in 'git push --help' for details.\n";
    const PUSH_OK: &str = "\nCounting objects: 3, done.\nDelta compression using up to 4 threads.\nCompressing objects: 100% (2/2), done.\nWriting objects: 100% (3/3), 282 bytes | 0 bytes/s, done.\nTotal 3 (delta 0), reused 0 (delta 0)\nTo /tmp/bar\n   514eed3..f269c79  master -> master\n";

    #[test]
    fn add_stages_a_file_git_does_not_know() {
        let output = "error: pathspec 'Cargo.toml' did not match any file(s) known to git.";
        assert!(offers(
            "git commit Cargo.toml",
            output,
            "git add -- Cargo.toml && git commit Cargo.toml"
        ));
        assert!(offers(
            "git submodule update Cargo.toml",
            output,
            "git add -- Cargo.toml && git submodule update Cargo.toml"
        ));
    }

    #[test]
    fn add_needs_the_file_to_exist() {
        let output = "error: pathspec 'welp-missing-file' did not match any file(s) known to git.";
        assert!(
            !fixes_for("git commit welp-missing-file", output)
                .iter()
                .any(|fix| fix.contains("git add"))
        );
    }

    #[test]
    fn add_force_for_ignored_files() {
        let output = "The following paths are ignored by one of your .gitignore files:\ndist/app.js\ndist/background.js\ndist/options.js\nUse -f if you really want to add them.\n";
        assert!(offers(
            "git add dist/*.js",
            output,
            "git add --force dist/*.js"
        ));
        assert!(!rule_matches("git_add_force", "git add dist/*.js", ""));
    }

    #[test]
    fn branch_exists_offers_only_safe_fixes() {
        let fixes = fixes_for(
            "git branch foo",
            "fatal: A branch named 'foo' already exists.",
        );
        assert_eq!(
            fixes,
            [
                "git branch -d foo && git branch foo",
                "git branch -d foo && git checkout -b foo",
                "git checkout foo",
            ]
        );
        assert!(!fixes.iter().any(|fix| fix.contains("-D")));
        assert!(offers(
            "git checkout -b bar",
            "fatal: a branch named 'bar' already exists",
            "git checkout bar"
        ));
        assert!(offers(
            "git checkout -b \"let's-push-this\"",
            "fatal: A branch named 'let's-push-this' already exists.",
            "git checkout 'let'\\''s-push-this'"
        ));
        assert!(!rule_matches("git_branch_exists", "git branch foo", ""));
    }

    #[test]
    fn clone_clone() {
        let output =
            "\nfatal: Too many arguments.\n\nusage: git clone [<options>] [--] <repo> [<dir>]\n";
        assert_eq!(
            fixes_for("git clone git clone foo", output),
            ["git clone foo"]
        );
        assert!(!rule_matches(
            "git_clone_git_clone",
            "git clone foo bar baz",
            output
        ));
        assert!(!rule_matches("git_clone_git_clone", "git clone foo", ""));
    }

    #[test]
    fn hook_bypass() {
        for (script, fix) in [
            ("git am", "git am --no-verify"),
            ("git commit", "git commit --no-verify"),
            (
                "git commit -m 'foo bar'",
                "git commit --no-verify -m 'foo bar'",
            ),
            ("git push", "git push --no-verify"),
            ("git push -u foo bar", "git push --no-verify -u foo bar"),
        ] {
            assert!(offers(script, "", fix), "{script}");
        }
        for script in ["git add foo", "git status", "git diff foo bar"] {
            assert!(!rule_matches("git_hook_bypass", script, ""), "{script}");
        }
        assert!(!rule_matches(
            "git_hook_bypass",
            "git commit --no-verify",
            ""
        ));
    }

    #[test]
    fn hook_bypass_comes_last() {
        let output = "fatal: The current branch feature has no upstream branch.\n    git push --set-upstream origin feature\n";
        let fixes = fixes_for("git push", output);
        assert_eq!(fixes[0], "git push --set-upstream origin feature");
        assert_eq!(fixes.last().unwrap(), "git push --no-verify");
    }

    #[test]
    fn main_and_master() {
        let master = "error: pathspec 'master' did not match any file(s) known to git";
        let main = "error: pathspec 'main' did not match any file(s) known to git";
        assert!(offers("git checkout main", main, "git checkout master"));
        assert!(offers("git checkout master", master, "git checkout main"));
        assert!(offers(
            "git show master",
            "fatal: ambiguous argument 'master': unknown revision",
            "git show main"
        ));
        assert!(offers(
            "git merge origin/master",
            "merge: 'master' - not something we can merge",
            "git merge origin/main"
        ));
        assert!(!offers(
            "git checkout wibble",
            "error: pathspec 'wibble' did not match any file(s) known to git",
            "git checkout main"
        ));
        assert!(!rule_matches("git_main_master", "git checkout master", ""));
        // a longer branch name is not the default branch
        assert!(!offers(
            "git checkout master-fix",
            master,
            "git checkout main-fix"
        ));
    }

    #[test]
    fn merge_the_remote_branch() {
        let output =
            "merge: local - not something we can merge\n\nDid you mean this?\n\tremote/local";
        assert!(offers("git merge local", output, "git merge remote/local"));
        assert!(offers(
            "git merge -m \"test\" local",
            output,
            "git merge -m \"test\" remote/local"
        ));
        assert!(offers(
            "git merge -m \"test local\" local",
            output,
            "git merge -m \"test local\" remote/local"
        ));
        assert!(!offers("git merge master", "", "git merge remote/local"));
        assert!(!offers("ls", output, "ls remote/local"));
    }

    #[test]
    fn pull_before_a_rejected_push() {
        for output in [PUSH_BEHIND, PUSH_REMOTE_WORK] {
            assert!(offers("git push", output, "git pull && git push"));
            assert!(offers(
                "git push nvbn",
                output,
                "git pull nvbn && git push nvbn"
            ));
            assert!(offers(
                "git push nvbn master",
                output,
                "git pull nvbn master && git push nvbn master"
            ));
        }
        for output in [PUSH_OK, "Everything up-to-date"] {
            assert!(!offers("git push", output, "git pull && git push"));
        }
    }

    #[test]
    fn rebase_skips_an_empty_patch() {
        let output = "Applying: Test commit\nNo changes - did you forget to use 'git add'?\nIf there is nothing left to stage, chances are that something else\nalready introduced the same changes; you might want to skip this patch.\n\nWhen you have resolved this problem, run \"git rebase --continue\".\nIf you prefer to skip this patch, run \"git rebase --skip\" instead.\nTo check out the original branch and stop rebasing, run \"git rebase --abort\".\n\n";
        assert_eq!(
            fixes_for("git rebase --continue", output),
            ["git rebase --skip"]
        );
        assert!(!rule_matches(
            "git_rebase_no_changes",
            "git rebase --continue",
            ""
        ));
        assert!(!rule_matches(
            "git_rebase_no_changes",
            "git rebase --skip",
            ""
        ));
    }

    #[test]
    fn remote_set_url_becomes_add() {
        let url = "git@github.com:nvbn/thefuck.git";
        for output in ["fatal: No such remote", "error: No such remote 'origin'"] {
            assert!(offers(
                &format!("git remote set-url origin {url}"),
                output,
                &format!("git remote add origin {url}")
            ));
        }
        for script in [
            "git remote set-url origin url",
            "git remote add origin url",
            "git remote remove origin",
            "git remote prune origin",
            "git remote set-branches origin branch",
        ] {
            assert!(
                !rule_matches("git_remote_seturl_add", script, ""),
                "{script}"
            );
        }
    }

    #[test]
    fn rm_with_local_modifications_keeps_the_file() {
        let output = "error: the following file has local modifications:\n    bar\n(use --cached to keep the file, or -f to force removal)";
        assert_eq!(
            fixes_for("git rm foo bar", output),
            ["git rm --cached foo bar"]
        );
        // -f would throw the modifications away
        assert!(
            !fixes_for("git rm foo", output)
                .iter()
                .any(|f| f.contains(" -f"))
        );
        assert!(!rule_matches(
            "git_rm_local_modifications",
            "git rm foo",
            ""
        ));
    }

    #[test]
    fn rm_recursive() {
        assert_eq!(
            fixes_for(
                "git rm foo",
                "fatal: not removing 'foo' recursively without -r"
            ),
            ["git rm -r foo"]
        );
        assert!(offers(
            "git rm foo bar",
            "fatal: not removing 'foo bar' recursively without -r",
            "git rm -r foo bar"
        ));
        assert!(!rule_matches("git_rm_recursive", "git rm foo", ""));
    }

    #[test]
    fn rm_with_staged_changes_keeps_the_file() {
        let output = "error: the following file has changes staged in the index:\n    bar\n(use --cached to keep the file, or -f to force removal)";
        assert_eq!(
            fixes_for("git rm foo bar", output),
            ["git rm --cached foo bar"]
        );
        assert!(
            !fixes_for("git rm foo", output)
                .iter()
                .any(|f| f.contains(" -f"))
        );
        assert!(!rule_matches("git_rm_staged", "git rm", ""));
    }

    #[test]
    fn stash_before_retrying() {
        let cherry_pick = "error: Your local changes would be overwritten by cherry-pick.\nhint: Commit your changes or stash them to proceed.\nfatal: cherry-pick failed";
        let rebase =
            "Cannot rebase: Your index contains uncommitted changes.\nPlease commit or stash them.";
        assert!(offers(
            "git cherry-pick a1b2c3d",
            cherry_pick,
            "git stash && git cherry-pick a1b2c3d"
        ));
        assert!(offers(
            "git rebase -i HEAD~7",
            rebase,
            "git stash && git rebase -i HEAD~7"
        ));
        assert!(!rule_matches("git_stash", "git cherry-pick a1b2c3d", ""));
        assert!(!rule_matches("git_stash", "git rebase -i HEAD~7", ""));
    }
}

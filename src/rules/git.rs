use std::sync::LazyLock;

use regex::Regex;

use super::Rule;
use crate::command::Command;

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "git_push",
            description: "Sets the upstream when a new branch has none",
            priority: 900,
            matches: |c| {
                c.is("git") && has_word(c, "push") && c.output.contains("git push --set-upstream")
            },
            fixes: git_push,
        },
        Rule {
            name: "git_pull",
            description: "Sets the tracking branch, then pulls",
            priority: 900,
            matches: |c| {
                c.is("git") && has_word(c, "pull") && c.output.contains("no tracking information")
            },
            fixes: git_pull,
        },
        Rule {
            name: "git_not_command",
            description: "Fixes a mistyped git subcommand",
            priority: 900,
            matches: |c| c.is("git") && c.output.contains("is not a git command"),
            fixes: git_not_command,
        },
        Rule {
            name: "git_checkout",
            description: "Creates the branch when it does not exist yet",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && (has_word(c, "checkout") || has_word(c, "switch"))
                    && (c.output.contains("did not match any file(s) known to git")
                        || c.output.contains("invalid reference"))
            },
            fixes: git_checkout,
        },
        Rule {
            name: "git_branch_delete",
            description: "Force-deletes a branch that is not merged",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "branch")
                    && has_word(c, "-d")
                    && c.output.contains("is not fully merged")
            },
            fixes: |c| vec![c.replace_word("-d", "-D")],
        },
        Rule {
            name: "git_commit_add",
            description: "Stages changed files when a commit has nothing staged",
            priority: 1000,
            matches: |c| {
                c.is("git")
                    && has_word(c, "commit")
                    && c.output.contains("no changes added to commit")
            },
            fixes: |c| vec![c.replace_word("commit", "commit -a")],
        },
    ]
}

pub(super) fn has_word(command: &Command, word: &str) -> bool {
    command.parts.iter().any(|part| part == word)
}

/// Git prints the exact command to run; extra flags such as `--force` are kept.
fn git_push(command: &Command) -> Vec<String> {
    let Some(line) = command
        .output
        .lines()
        .find(|line| line.contains("git push --set-upstream"))
    else {
        return Vec::new();
    };
    let mut fix = line.trim().to_string();
    for flag in [
        "--force",
        "-f",
        "--force-with-lease",
        "--tags",
        "--no-verify",
    ] {
        if has_word(command, flag) && !fix.contains(flag) {
            fix.push(' ');
            fix.push_str(flag);
        }
    }
    vec![fix]
}

static SET_UPSTREAM: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git branch --set-upstream-to=(\S+) (\S+)").unwrap());

fn git_pull(command: &Command) -> Vec<String> {
    let Some(found) = SET_UPSTREAM.captures(&command.output) else {
        return Vec::new();
    };
    let branch = &found[2];
    let remote = found[1].replace("<branch>", branch);
    vec![format!(
        "git branch --set-upstream-to={remote} {branch} && {}",
        command.script
    )]
}

static MISTYPED: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"git: '([^']+)' is not a git command").unwrap());

fn git_not_command(command: &Command) -> Vec<String> {
    let Some(found) = MISTYPED.captures(&command.output) else {
        return Vec::new();
    };
    let wrong = &found[1];
    let suggestions = command
        .output
        .lines()
        .skip_while(|line| !line.to_lowercase().contains("most similar command"))
        .skip(1)
        .take_while(|line| line.starts_with('\t') || line.starts_with("    "))
        .map(str::trim);
    suggestions
        .map(|right| command.replace_word(wrong, right))
        .collect()
}

fn git_checkout(command: &Command) -> Vec<String> {
    let switching = has_word(command, "switch");
    let verb = if switching { "switch" } else { "checkout" };
    let after_verb = command
        .parts
        .iter()
        .skip_while(|part| *part != verb)
        .skip(1);
    let Some(branch) = after_verb.filter(|part| !part.starts_with('-')).last() else {
        return Vec::new();
    };
    let create = if switching {
        "switch -c"
    } else {
        "checkout -b"
    };
    vec![format!("git {create} {}", shell_words::quote(branch))]
}

#[cfg(test)]
mod tests {
    use crate::rules::fixes_for;

    #[test]
    fn push_sets_upstream() {
        let output = "fatal: The current branch feature has no upstream branch.\nTo push the current branch and set the remote as upstream, use\n\n    git push --set-upstream origin feature\n";
        // first, ahead of the weaker guesses that follow it
        assert_eq!(
            fixes_for("git push", output)[0],
            "git push --set-upstream origin feature"
        );
        assert_eq!(
            fixes_for("git push --force", output)[0],
            "git push --set-upstream origin feature --force"
        );
    }

    #[test]
    fn pull_sets_tracking() {
        let output = "There is no tracking information for the current branch.\n    git branch --set-upstream-to=origin/<branch> main\n";
        assert_eq!(
            fixes_for("git pull", output),
            ["git branch --set-upstream-to=origin/main main && git pull"]
        );
    }

    #[test]
    fn not_a_git_command() {
        let output = "git: 'comit' is not a git command. See 'git --help'.\n\nThe most similar command is\n\tcommit\n";
        assert_eq!(
            fixes_for("git comit -m 'wip'", output),
            ["git commit -m wip"]
        );
    }

    #[test]
    fn checkout_creates_branch() {
        let output = "error: pathspec 'new-ui' did not match any file(s) known to git";
        assert_eq!(
            fixes_for("git checkout new-ui", output),
            ["git checkout -b new-ui"]
        );
        assert_eq!(
            fixes_for("git switch new-ui", "fatal: invalid reference: new-ui"),
            ["git switch -c new-ui"]
        );
    }

    #[test]
    fn branch_force_delete() {
        let output = "error: The branch 'old' is not fully merged.";
        assert_eq!(
            fixes_for("git branch -d old", output),
            ["git branch -D old"]
        );
    }
}

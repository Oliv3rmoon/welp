//! More shell, file and system rules ported from thefuck.
//!
//! Where welp does something different from thefuck on purpose:
//!
//! - `dirty_untar` and `dirty_unzip` offer the same fix, but not thefuck's
//!   side effect, which deleted the files the first extraction had scattered
//!   around. That would be a hidden `rm`, so it is left out.
//! - `ssh_known_hosts` used to rerun the same command after quietly deleting
//!   the offending line from `known_hosts`. The deletion is now in the shown
//!   command: `ssh-keygen -R host && ssh host`. The rule is off by default.
//! - `port_already_in_use` asks `lsof` only for the process that is
//!   listening on the port, not for clients connected to it, so it never
//!   offers to kill a browser that happens to talk to the port.
//! - `hostscli` swaps the quoted word hostscli complained about. thefuck
//!   tried to swap the whole error sentence, which is never in the command.
//! - `ls_lah`, `man` and `whois` match every command of theirs, so they sit
//!   below the rules that read an error. `history` and `long_form_help` are
//!   the other weak guesses.
//! - `mv`, `rm` and `ssh` are never run again, so their output is empty;
//!   `cp_create_destination`, `no_such_file` and `rm_root` look at the
//!   files (or, for `rm`, at the failsafe GNU rm applies) instead.
//!   `ssh_known_hosts` has nothing to look at, so it only fires once `ssh`
//!   is taken out of `no_rerun`. Nor are commands that start with `sudo`
//!   run again, so no rule sees an error from one.
//! - Rules that thefuck limits to one program (`for_app`) leave `sudo ls`
//!   alone; only the ones thefuck gives `sudo_support` skip a `sudo`.

use std::fs;
use std::io::{Read, Seek, SeekFrom};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::{Rule, listing, replace_arg, replace_program};
use crate::command::Command;

/// Rules in this file that are off unless the user enables them.
pub const DISABLED: &[&str] = &[
    // kills the process that holds the port
    "port_already_in_use",
    // runs rm on / past the failsafe
    "rm_root",
    // removes a host key from known_hosts
    "ssh_known_hosts",
];

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "cd_cs",
            description: "cs to cd, the keys sit side by side",
            priority: 900,
            matches: |c| c.parts.first().is_some_and(|first| first == "cs"),
            fixes: |c| {
                c.script
                    .strip_prefix("cs")
                    .map(|rest| format!("cd{rest}"))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "cp_create_destination",
            description: "Creates the destination folder, then copies or moves into it",
            priority: 1000,
            matches: |c| (is_app(c, "cp") || is_app(c, "mv")) && destination_is_missing(c),
            fixes: |c| {
                c.parts
                    .last()
                    .map(|dest| format!("mkdir -p {} && {}", crate::command::quote(dest), c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "dirty_untar",
            description: "Extracts a tar archive into a folder of its own",
            priority: 1000,
            matches: |c| untar_folder(c).is_some(),
            fixes: |c| {
                untar_folder(c)
                    .map(|dir| format!("mkdir -p {dir} && {} -C {dir}", c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "dirty_unzip",
            description: "Extracts a zip with many files into a folder of its own",
            priority: 1000,
            matches: |c| unzip_folder(c, zip_entries).is_some(),
            fixes: |c| {
                unzip_folder(c, zip_entries)
                    .map(|dir| format!("{} -d {dir}", c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "dry",
            description: "Drops a word you typed twice",
            priority: 900,
            matches: |c| c.parts.len() >= 2 && c.parts[0] == c.parts[1],
            fixes: |c| {
                c.script
                    .split_once(char::is_whitespace)
                    .map(|(_, rest)| rest.trim_start().to_string())
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "fix_alt_space",
            description: "Swaps a non-breaking space (Alt+Space) for a normal one",
            priority: 1000,
            matches: |c| {
                c.output_lower().contains("command not found") && c.script.contains('\u{a0}')
            },
            fixes: |c| vec![c.script.replace('\u{a0}', " ")],
        },
        Rule {
            name: "fix_file",
            description: "Opens $EDITOR at the line an error points to, then runs the command again",
            priority: 1000,
            matches: |c| editor().is_some() && error_location(&c.output, is_file).is_some(),
            fixes: |c| {
                let (Some(editor), Some((file, line))) =
                    (editor(), error_location(&c.output, is_file))
                else {
                    return Vec::new();
                };
                vec![open_at(&editor, &file, &line, &c.script)]
            },
        },
        Rule {
            name: "grep_arguments_order",
            description: "Puts the file after the pattern in grep",
            priority: 1000,
            matches: |c| {
                (is_app(c, "grep") || is_app(c, "egrep"))
                    && c.output.contains(": No such file or directory")
                    && existing_operand(c, exists_as_file_or_dir).is_some()
            },
            fixes: |c| {
                existing_operand(c, exists_as_file_or_dir)
                    .map(|file| move_last(&c.parts, file))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "history",
            description: "Tries the closest command from your history",
            priority: 1500,
            matches: |c| !history_matches(c, is_known_program).is_empty(),
            fixes: |c| {
                history_matches(c, is_known_program)
                    .into_iter()
                    .take(1)
                    .collect()
            },
        },
        Rule {
            name: "hostscli",
            description: "Fixes a misspelled hostscli command, or lists the websites it knows",
            priority: 1000,
            matches: |c| {
                c.is("hostscli")
                    && (c.output.contains("Error: No such command")
                        || c.output.contains(HOSTSCLI_NO_WEBSITE))
            },
            fixes: hostscli,
        },
        Rule {
            name: "ifconfig_device_not_found",
            description: "Uses an interface ifconfig lists instead of the one you typed",
            priority: 1000,
            matches: |c| {
                is_app(c, "ifconfig")
                    && c.output
                        .contains("error fetching interface information: Device not found")
            },
            fixes: |c| match listing("ifconfig", &["-a"]) {
                Some(all_interfaces) => ifconfig_fixes(c, &all_interfaces),
                None => Vec::new(),
            },
        },
        Rule {
            name: "ln_s_order",
            description: "Puts the link name last in ln -s",
            priority: 1000,
            matches: |c| ln_fix(c, path_exists).is_some(),
            fixes: |c| ln_fix(c, path_exists).into_iter().collect(),
        },
        Rule {
            name: "long_form_help",
            description: "Uses --help, or the help command the tool suggests",
            priority: 1400,
            matches: |c| HELP_HINT.is_match(&c.output) || c.output.contains("--help"),
            fixes: |c| match HELP_HINT.captures(&c.output) {
                Some(hint) => vec![hint[1].to_string()],
                None => vec![replace_arg(&c.script, "-h", "--help")],
            },
        },
        Rule {
            name: "ls_all",
            description: "Adds -A when ls shows nothing, to see hidden files",
            priority: 1000,
            matches: |c| is_app(c, "ls") && c.output.trim().is_empty(),
            fixes: |c| vec![replace_program(&c.script, "ls -A")],
        },
        Rule {
            name: "ls_lah",
            description: "Lists with -lah",
            priority: 1200,
            matches: |c| is_app(c, "ls") && !c.script.contains("ls -"),
            fixes: |c| vec![replace_program(&c.script, "ls -lah")],
        },
        Rule {
            name: "man",
            description: "Tries another man section, or the --help of the tool",
            priority: 1100,
            matches: |c| is_app(c, "man") && !args(c).is_empty(),
            fixes: man,
        },
        Rule {
            name: "mercurial",
            description: "Uses the hg command hg suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, "hg")
                    && ((c.output.contains("hg: unknown command")
                        && c.output.contains("(did you mean one of "))
                        || (c.output.contains("hg: command '")
                            && c.output.contains("' is ambiguous:")))
            },
            fixes: mercurial,
        },
        Rule {
            name: "no_such_file",
            description: "Creates the folder cp or mv could not find, then runs the command again",
            priority: 1000,
            matches: |c| missing_folder(c).is_some(),
            fixes: |c| {
                missing_folder(c)
                    .map(|dir| format!("mkdir -p {} && {}", crate::command::quote(&dir), c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "open",
            description: "Opens a web address with http://, or creates the missing file or folder",
            priority: 1000,
            matches: |c| {
                ["open", "xdg-open", "gnome-open", "kde-open"]
                    .iter()
                    .any(|program| is_app(c, program))
                    && (is_arg_url(c) || file_does_not_exist(&c.output))
            },
            fixes: open,
        },
        Rule {
            name: "path_from_history",
            description: "Swaps a path that is not there for a longer one from your history",
            priority: 800,
            matches: |c| missing_path(c).is_some(),
            fixes: |c| {
                missing_path(c)
                    .map(|path| paths_from_history(c, &path, is_known_program, exists_with_tilde))
                    .unwrap_or_default()
            },
        },
        Rule {
            name: "port_already_in_use",
            description: "Kills the process that holds the port, then runs the command again",
            priority: 1000,
            matches: |c| {
                used_port(&c.output)
                    .and_then(|port| listener_pid(&port))
                    .is_some()
            },
            fixes: |c| {
                used_port(&c.output)
                    .and_then(|port| listener_pid(&port))
                    .map(|pid| format!("kill {pid} && {}", c.script))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "prove_recursively",
            description: "Adds -r to prove to look inside folders",
            priority: 1000,
            matches: |c| {
                is_app(c, "prove")
                    && c.output.contains("NOTESTS")
                    && !args(c).iter().any(|part| is_recursive(part))
                    && args(c)
                        .iter()
                        .any(|part| !part.starts_with('-') && Path::new(part).is_dir())
            },
            fixes: |c| vec![replace_program(&c.script, "prove -r")],
        },
        Rule {
            name: "remove_shell_prompt_literal",
            description: "Drops the $ copied from a documentation code block",
            priority: 1000,
            matches: |c| {
                c.output.contains("$: command not found") && PROMPT_LITERAL.is_match(&c.script)
            },
            fixes: |c| vec![c.script.trim_start_matches(['$', ' ']).to_string()],
        },
        Rule {
            name: "remove_trailing_cedilla",
            description: "Drops a ç typed at the end",
            priority: 1000,
            matches: |c| c.script.ends_with('ç'),
            fixes: |c| {
                c.script
                    .strip_suffix('ç')
                    .map(String::from)
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "rm_root",
            description: "Adds --no-preserve-root to rm on /",
            priority: 1000,
            matches: |c| rm_root(c, cfg!(target_os = "linux")),
            fixes: |c| vec![format!("{} --no-preserve-root", c.script)],
        },
        Rule {
            name: "scm_correction",
            description: "Uses git or hg, whichever repository this folder holds",
            priority: 1000,
            matches: |c| wrong_scm(c, actual_scm).is_some(),
            fixes: |c| {
                wrong_scm(c, actual_scm)
                    .map(|scm| replace_program(&c.script, scm))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "sed_unterminated_s",
            description: "Closes the s/// command with a slash",
            priority: 1000,
            matches: |c| is_app(c, "sed") && c.output.contains("unterminated `s' command"),
            fixes: |c| {
                shell_words::split(&c.script)
                    .map(|parts| close_substitutions(&parts))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "ssh_known_hosts",
            description: "Removes the changed host key with ssh-keygen -R, then connects again",
            priority: 1000,
            matches: |c| (is_app(c, "ssh") || is_app(c, "scp")) && host_key_changed(&c.output),
            fixes: ssh_known_hosts,
        },
        Rule {
            name: "switch_lang",
            description: "Retypes a command written in another keyboard layout",
            priority: 1000,
            matches: |c| {
                c.output.contains("not found")
                    && switched_layout(&c.script, &alias_names()).is_some()
            },
            fixes: |c| {
                switched_layout(&c.script, &alias_names())
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "systemctl",
            description: "Puts the action before the service in systemctl",
            priority: 1000,
            matches: |c| c.is("systemctl") && misordered_systemctl(c),
            fixes: |c| {
                let mut parts = Vec::from(skip_sudo(c));
                let [.., service, action] = parts.as_mut_slice() else {
                    return Vec::new();
                };
                std::mem::swap(service, action);
                vec![format!("{}{}", sudo_prefix(c), crate::command::join(parts))]
            },
        },
        Rule {
            name: "tmux",
            description: "Picks the full tmux command out of the ones it says it could be",
            priority: 1000,
            matches: |c| {
                is_app(c, "tmux")
                    && c.output.contains("ambiguous command:")
                    && c.output.contains("could be:")
            },
            fixes: |c| {
                let Some(found) = TMUX_AMBIGUOUS.captures(&c.output) else {
                    return Vec::new();
                };
                let suggestions: Vec<String> = found[2]
                    .split(',')
                    .map(|name| name.trim().to_string())
                    .collect();
                replace_command(&c.script, &found[1], &suggestions)
            },
        },
        Rule {
            name: "unknown_command",
            description: "Uses the command a tool says you probably meant (hdfs and others)",
            priority: 1000,
            matches: |c| {
                UNKNOWN_COMMAND.is_match(&c.output)
                    && DID_YOU_MEAN_UNTIL_QUESTION.is_match(&c.output)
            },
            fixes: |c| {
                let Some(broken) = UNKNOWN_COMMAND.captures(&c.output) else {
                    return Vec::new();
                };
                // the word is the last thing printed before the colon
                let broken = broken[1].rsplit('\n').next().unwrap_or("").trim();
                let meant: Vec<String> = DID_YOU_MEAN_UNTIL_QUESTION
                    .captures_iter(&c.output)
                    .map(|found| found[1].to_string())
                    .collect();
                replace_command(&c.script, broken, &meant)
            },
        },
        Rule {
            name: "whois",
            description: "Asks whois about the domain instead of the whole address",
            priority: 1100,
            matches: |c| is_app(c, "whois") && !args(c).is_empty(),
            fixes: whois,
        },
        Rule {
            name: "wrong_hyphen_before_subcommand",
            description: "Swaps the hyphen in git-log style commands for a space",
            priority: 1300,
            matches: |c| wrong_hyphen(c, is_executable_on_path),
            fixes: |c| vec![c.script.replacen('-', " ", 1)],
        },
    ]
}

// ---------------------------------------------------------------------------
// Shared helpers. `more_git` uses the string-matching ones too.

/// How alike two strings are, from 0 to 1, the way Python's difflib scores
/// them: twice the characters in the longest runs they share (found again on
/// each side of a run), over the length of both. thefuck sorts its guesses
/// by this number, so welp does the same to offer them in the same order.
pub(super) fn ratio(a: &str, b: &str) -> f64 {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.is_empty() && b.is_empty() {
        return 1.0;
    }
    2.0 * matching_chars(&a, &b) as f64 / (a.len() + b.len()) as f64
}

fn matching_chars(a: &[char], b: &[char]) -> usize {
    let mut total = 0;
    let mut ranges = vec![(0, a.len(), 0, b.len())];
    while let Some((a_low, a_high, b_low, b_high)) = ranges.pop() {
        let (i, j, size) = longest_match(a, b, a_low, a_high, b_low, b_high);
        if size == 0 {
            continue;
        }
        total += size;
        if a_low < i && b_low < j {
            ranges.push((a_low, i, b_low, j));
        }
        if i + size < a_high && j + size < b_high {
            ranges.push((i + size, a_high, j + size, b_high));
        }
    }
    total
}

/// The longest run both share inside the given ranges: where it starts in
/// each, and how long. The earliest one wins a tie, as in difflib.
fn longest_match(
    a: &[char],
    b: &[char],
    a_low: usize,
    a_high: usize,
    b_low: usize,
    b_high: usize,
) -> (usize, usize, usize) {
    let (mut best_i, mut best_j, mut best_size) = (a_low, b_low, 0);
    // run[j + 1] is how long a run ends at a[i - 1], b[j]
    let mut run = vec![0usize; b.len() + 1];
    for (i, ch) in a.iter().enumerate().take(a_high).skip(a_low) {
        let mut next = vec![0usize; b.len() + 1];
        for j in b_low..b_high {
            if *ch == b[j] {
                let size = run[j] + 1;
                next[j + 1] = size;
                if size > best_size {
                    (best_i, best_j, best_size) = (i + 1 - size, j + 1 - size, size);
                }
            }
        }
        run = next;
    }
    (best_i, best_j, best_size)
}

/// Up to `n` of the possibilities that look like `word`, best first. A tie
/// goes to the one that sorts later, as in Python.
pub(super) fn close_matches<S: AsRef<str>>(
    word: &str,
    possibilities: &[S],
    n: usize,
    cutoff: f64,
) -> Vec<String> {
    let mut scored: Vec<(f64, &str)> = possibilities
        .iter()
        .map(|possibility| (ratio(possibility.as_ref(), word), possibility.as_ref()))
        .filter(|(score, _)| *score >= cutoff)
        .collect();
    scored.sort_by(|a, b| b.0.total_cmp(&a.0).then_with(|| b.1.cmp(a.1)));
    scored
        .into_iter()
        .take(n)
        .map(|(_, possibility)| possibility.to_string())
        .collect()
}

/// The best of the possibilities (a score of at least `cutoff`), or the
/// first one when `fall_back` says so and none is close enough.
pub(super) fn closest<S: AsRef<str>>(
    word: &str,
    possibilities: &[S],
    cutoff: f64,
    fall_back: bool,
) -> Option<String> {
    close_matches(word, possibilities, 1, cutoff)
        .into_iter()
        .next()
        .or_else(|| {
            fall_back
                .then(|| {
                    possibilities
                        .first()
                        .map(|first| first.as_ref().to_string())
                })
                .flatten()
        })
}

/// The script with the word `broken` swapped for each of the three
/// possibilities that look most like it.
pub(super) fn replace_command<S: AsRef<str>>(
    script: &str,
    broken: &str,
    matched: &[S],
) -> Vec<String> {
    close_matches(broken, matched, 3, 0.1)
        .iter()
        .map(|new| replace_arg(script, broken, new.trim()))
        .collect()
}

/// The last word of each line between two lines that hold one of the
/// separators (and of the first of them, as thefuck does).
pub(super) fn matched_commands(output: &str, separators: &[&str]) -> Vec<String> {
    let mut inside = false;
    let mut found = Vec::new();
    for line in output.split('\n') {
        if separators.iter().any(|separator| line.contains(separator)) {
            inside = !inside;
        }
        if inside && !line.is_empty() {
            found.push(line.rsplit(' ').next().unwrap_or("").to_string());
        }
    }
    found
}

/// The command's words without a leading `sudo`.
fn skip_sudo(c: &Command) -> &[String] {
    match c.parts.first().map(String::as_str) {
        Some("sudo") => &c.parts[1..],
        _ => &c.parts,
    }
}

fn sudo_prefix(c: &Command) -> &'static str {
    if c.parts.first().is_some_and(|first| first == "sudo") {
        "sudo "
    } else {
        ""
    }
}

/// Whether the command is a call to this program. A leading `sudo` does not
/// count, as in thefuck's `for_app`: only the rules that take sudo into
/// account (`sudo_support` there) use `Command::is`.
pub(super) fn is_app(c: &Command, program: &str) -> bool {
    c.parts.first().is_some_and(|first| first == program)
}

/// Everything after the program.
pub(super) fn args(c: &Command) -> &[String] {
    skip_sudo(c).get(1..).unwrap_or(&[])
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    return path
        .metadata()
        .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0);
    // Windows has no execute bit; any file can be run by its extension.
    #[cfg(not(unix))]
    return path.is_file();
}

fn is_executable_on_path(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.contains('/') {
        return is_executable(Path::new(name));
    }
    std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| is_executable(&dir.join(name))))
}

const BUILTINS: &[&str] = &[
    ".",
    ":",
    "[",
    "alias",
    "bg",
    "bind",
    "break",
    "builtin",
    "cd",
    "command",
    "continue",
    "declare",
    "dirs",
    "disown",
    "echo",
    "eval",
    "exec",
    "exit",
    "export",
    "fc",
    "fg",
    "getopts",
    "hash",
    "help",
    "history",
    "jobs",
    "kill",
    "let",
    "local",
    "logout",
    "mapfile",
    "popd",
    "printf",
    "pushd",
    "pwd",
    "read",
    "readarray",
    "readonly",
    "return",
    "set",
    "shift",
    "shopt",
    "source",
    "suspend",
    "test",
    "times",
    "trap",
    "type",
    "typeset",
    "ulimit",
    "umask",
    "unalias",
    "unset",
    "wait",
];

/// A program that exists: a shell builtin, or an executable on the PATH.
fn is_known_program(name: &str) -> bool {
    BUILTINS.contains(&name) || is_executable_on_path(name)
}

fn exists_as_file_or_dir(path: &str) -> bool {
    let path = Path::new(path);
    path.is_file() || path.is_dir()
}

// ---------------------------------------------------------------------------
// cp_create_destination, no_such_file

/// For `cp` and `mv`: the destination (the last word) and the folder it
/// should sit in, when that folder is not there. Used when the output is
/// empty, which is always the case for `mv`, which is never run again.
fn missing_folder_on_disk(c: &Command) -> Option<(String, String)> {
    let operands: Vec<&String> = args(c).iter().filter(|a| !a.starts_with('-')).collect();
    let (destination, sources) = operands.split_last()?;
    if sources.is_empty() || !sources.iter().all(|source| Path::new(source).exists()) {
        return None;
    }
    let folder = folder_of(destination)?;
    (!Path::new(folder).exists()).then(|| (destination.to_string(), folder.to_string()))
}

/// `a/b` for `a/b/c` and `a/b/`, nothing for a bare name or `/c`.
fn folder_of(path: &str) -> Option<&str> {
    path.rfind('/')
        .filter(|slash| *slash > 0)
        .map(|slash| &path[..slash])
}

fn destination_is_missing(c: &Command) -> bool {
    if c.output.trim().is_empty() {
        return missing_folder_on_disk(c).is_some();
    }
    c.output.contains("No such file or directory")
        || (c.output.starts_with("cp: directory")
            && c.output.trim_end().ends_with("does not exist"))
}

static NO_SUCH_FILE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"mv: cannot move '[^']*' to '([^']*)': No such file or directory",
        r"mv: cannot move '[^']*' to '([^']*)': Not a directory",
        r"cp: cannot create regular file '([^']*)': No such file or directory",
        r"cp: cannot create regular file '([^']*)': Not a directory",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});

/// The folder cp or mv could not put its file in.
fn missing_folder(c: &Command) -> Option<String> {
    if c.output.trim().is_empty() {
        return (is_app(c, "cp") || is_app(c, "mv"))
            .then(|| missing_folder_on_disk(c))
            .flatten()
            .map(|(_, folder)| folder);
    }
    NO_SUCH_FILE
        .iter()
        .find_map(|pattern| pattern.captures(&c.output))
        .and_then(|found| folder_of(&found[1]).map(String::from))
}

// ---------------------------------------------------------------------------
// dirty_untar, dirty_unzip

const TAR_EXTENSIONS: [&str; 15] = [
    ".tar",
    ".tar.Z",
    ".tar.bz2",
    ".tar.gz",
    ".tar.lz",
    ".tar.lzma",
    ".tar.xz",
    ".taz",
    ".tb2",
    ".tbz",
    ".tbz2",
    ".tgz",
    ".tlz",
    ".txz",
    ".tz",
];

/// The quoted name of the folder to extract a tar archive into, when the
/// command extracts one without `-C`.
fn untar_folder(c: &Command) -> Option<String> {
    if !is_app(c, "tar") || c.script.contains("-C") {
        return None;
    }
    let extracts = c.script.contains("--extract")
        || c.script
            .split_whitespace()
            .nth(1)
            .is_some_and(|flags| !flags.starts_with("--") && flags.contains('x'));
    if !extracts {
        return None;
    }
    c.parts
        .iter()
        .find_map(|part| {
            TAR_EXTENSIONS
                .iter()
                .find_map(|extension| part.strip_suffix(extension))
        })
        .filter(|stem| !stem.is_empty())
        .map(crate::command::quote)
}

/// How many files a zip archive holds, read from the end of the archive.
fn zip_entry_count(tail: &[u8]) -> Option<usize> {
    const END_OF_CENTRAL_DIRECTORY: &[u8] = b"PK\x05\x06";
    let at = tail
        .windows(4)
        .rposition(|window| window == END_OF_CENTRAL_DIRECTORY)?;
    let count = tail.get(at + 10..at + 12)?;
    Some(usize::from(u16::from_le_bytes([count[0], count[1]])))
}

fn zip_entries(path: &str) -> Option<usize> {
    let mut file = fs::File::open(path).ok()?;
    // the end record is 22 bytes plus a comment of up to 65535
    let length = file.metadata().ok()?.len();
    file.seek(SeekFrom::Start(length.saturating_sub(22 + 65535)))
        .ok()?;
    let mut tail = Vec::new();
    file.read_to_end(&mut tail).ok()?;
    zip_entry_count(&tail)
}

/// The quoted name of the folder to unzip into, when the archive the command
/// names holds more than one file and the command has no `-d`.
fn unzip_folder(c: &Command, entries: impl Fn(&str) -> Option<usize>) -> Option<String> {
    if !is_app(c, "unzip") || c.script.contains("-d") {
        return None;
    }
    let archive = args(c).iter().find(|part| !part.starts_with('-'))?;
    let archive = if archive.ends_with(".zip") {
        archive.clone()
    } else {
        format!("{archive}.zip")
    };
    if entries(&archive)? <= 1 {
        return None;
    }
    let stem = archive.strip_suffix(".zip")?;
    (!stem.is_empty()).then(|| crate::command::quote(stem))
}

// ---------------------------------------------------------------------------
// fix_file

fn editor() -> Option<String> {
    std::env::var("EDITOR")
        .ok()
        .filter(|editor| !editor.is_empty())
}

fn is_file(path: &str) -> bool {
    Path::new(path).is_file()
}

/// The editor on the line of the file, then the command again.
fn open_at(editor: &str, file: &str, line: &str, script: &str) -> String {
    format!(
        "{editor} {} +{line} && {script}",
        crate::command::quote(file)
    )
}

/// How errors say where they happened; the first pattern that finds a file
/// that exists wins, so the order matters.
static ERROR_LOCATIONS: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        // js, node:
        "^    at {file}:{line}:{col}",
        // cargo:
        "^   {file}:{line}:{col}",
        // python, thefuck:
        "^  File \"{file}\", line {line}",
        // awk:
        "^awk: {file}:{line}:",
        // git:
        "^fatal: bad config file line {line} in {file}",
        // llc:
        "^llc: {file}:{line}:{col}:",
        // lua:
        "^lua: {file}:{line}:",
        // fish:
        r"^{file} \(line {line}\):",
        // bash, sh, ssh:
        "^{file}: line {line}: ",
        // cargo, clang, gcc, go, pep8, rustc:
        "^{file}:{line}:{col}",
        // ghc, make, ruby, zsh:
        "^{file}:{line}:",
        // perl:
        "at {file} line {line}",
    ]
    .iter()
    .map(|pattern| {
        let pattern = pattern
            // An optional `C:` so Windows paths keep their drive.
            .replace("{file}", r"(?P<file>(?:[A-Za-z]:)?[^:\n]+)")
            .replace("{line}", "(?P<line>[0-9]+)")
            .replace("{col}", "(?P<col>[0-9]+)");
        Regex::new(&format!("(?m){pattern}")).unwrap()
    })
    .collect()
});

/// The file and line an error message points to, if the file is there.
fn error_location(output: &str, is_file: impl Fn(&str) -> bool) -> Option<(String, String)> {
    ERROR_LOCATIONS.iter().find_map(|pattern| {
        let found = pattern.captures(output)?;
        is_file(&found["file"]).then(|| (found["file"].to_string(), found["line"].to_string()))
    })
}

// ---------------------------------------------------------------------------
// grep_arguments_order, ln_s_order

/// The first word after the program that is a real file or folder.
fn existing_operand(c: &Command, exists: impl Fn(&str) -> bool) -> Option<&String> {
    args(c).iter().find(|part| exists(part))
}

/// The words with `file` taken out and put last.
fn move_last(parts: &[String], file: &str) -> String {
    let mut moved = parts.to_vec();
    if let Some(at) = moved.iter().position(|part| part == file) {
        let file = moved.remove(at);
        moved.push(file);
    }
    crate::command::join(moved)
}

fn path_exists(path: &str) -> bool {
    Path::new(path).exists()
}

/// For `ln -s` that says the link exists: the command with the first word
/// that is a real path, which is the target when the arguments are the wrong
/// way round, moved to the end.
fn ln_fix(c: &Command, exists: impl Fn(&str) -> bool) -> Option<String> {
    let parts = skip_sudo(c);
    if parts.first().is_none_or(|first| first != "ln")
        || !parts
            .iter()
            .any(|part| part == "-s" || part == "--symbolic")
        || !c.output.contains("File exists")
    {
        return None;
    }
    let target = parts
        .iter()
        .find(|part| !matches!(part.as_str(), "ln" | "-s" | "--symbolic") && exists(part))?;
    Some(format!("{}{}", sudo_prefix(c), move_last(parts, target)))
}

// ---------------------------------------------------------------------------
// history, path_from_history

/// The recent commands worth trying again: not the one being fixed, and
/// starting with a program that exists.
fn valid_history<'a>(c: &'a Command, known: &dyn Fn(&str) -> bool) -> Vec<&'a str> {
    c.history
        .iter()
        .map(String::as_str)
        .filter(|line| *line != c.script && line.split(' ').next().is_some_and(known))
        .collect()
}

/// The commands from history that look like the one being fixed, best first.
fn history_matches(c: &Command, known: impl Fn(&str) -> bool) -> Vec<String> {
    close_matches(&c.script, &valid_history(c, &known), 3, 0.6)
}

static NO_SUCH_PATH: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"no such file or directory: (.*)\n?\z",
        r"cannot access '(.*)': No such file or directory",
        r": ([^:\n]*): No such file or directory",
        r"can't cd to (.*)\n?\z",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});

/// The path the error says is not there, if it is a word of the command.
fn missing_path(c: &Command) -> Option<String> {
    NO_SUCH_PATH.iter().find_map(|pattern| {
        let found = pattern.captures(&c.output)?;
        c.parts
            .iter()
            .any(|part| part == &found[1])
            .then(|| found[1].to_string())
    })
}

fn exists_with_tilde(path: &str) -> bool {
    match path.strip_prefix('~') {
        Some(rest) => {
            dirs::home_dir().is_some_and(|home| home.join(rest.trim_start_matches('/')).exists())
        }
        None => Path::new(path).exists(),
    }
}

/// The command with `missing` swapped for each absolute path from history
/// that ends with it, the most used path first.
fn paths_from_history(
    c: &Command,
    missing: &str,
    known: impl Fn(&str) -> bool,
    exists: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut counts: Vec<(String, usize)> = Vec::new();
    for line in valid_history(c, &known) {
        let words = shell_words::split(line)
            .unwrap_or_else(|_| line.split_whitespace().map(String::from).collect());
        for word in words.iter().skip(1) {
            if word.starts_with('/') || word.starts_with('~') {
                let path = word.strip_suffix('/').unwrap_or(word);
                match counts.iter().position(|(seen, _)| seen == path) {
                    Some(at) => counts[at].1 += 1,
                    None => counts.push((path.to_string(), 1)),
                }
            }
        }
    }
    // the most used first; a tie keeps the order the paths were first seen
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    counts
        .into_iter()
        .map(|(path, _)| path)
        .filter(|path| path.ends_with(missing) && exists(path))
        .map(|path| replace_arg(&c.script, missing, &path))
        .collect()
}

// ---------------------------------------------------------------------------
// hostscli, ifconfig_device_not_found, long_form_help, man, mercurial

const HOSTSCLI_NO_WEBSITE: &str = "hostscli.errors.WebsiteImportError";

fn hostscli(c: &Command) -> Vec<String> {
    if c.output.contains(HOSTSCLI_NO_WEBSITE) {
        return vec![format!("{}hostscli websites", sudo_prefix(c))];
    }
    static NO_SUCH_COMMAND: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"Error: No such command "([^"]*)""#).unwrap());
    let Some(found) = NO_SUCH_COMMAND.captures(&c.output) else {
        return Vec::new();
    };
    let commands = ["block", "unblock", "websites", "block_all", "unblock_all"];
    replace_command(&c.script, &found[1], &commands)
}

/// The interfaces `ifconfig -a` lists: the first word of each line that is
/// not indented.
fn interfaces(listing: &str) -> Vec<String> {
    listing
        .lines()
        .filter(|line| !line.is_empty() && !line.starts_with([' ', '\t']))
        .filter_map(|line| line.split([' ', '\t']).next())
        .map(|name| name.trim_end_matches(':').to_string())
        .filter(|name| !name.is_empty())
        .collect()
}

fn ifconfig_fixes(c: &Command, listing: &str) -> Vec<String> {
    let Some(wrong) = c
        .output
        .trim_start()
        .split(' ')
        .next()
        .and_then(|first| first.strip_suffix(':'))
    else {
        return Vec::new();
    };
    replace_command(&c.script, wrong, &interfaces(listing))
}

/// How tools suggest a help command: `Try 'grep --help' for more information.`
static HELP_HINT: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:Run|Try) '([^']+)'(?: or '[^']+')? for (?:details|more information).")
        .unwrap()
});

fn man(c: &Command) -> Vec<String> {
    if c.script.contains('3') {
        return vec![c.script.replace('3', "2")];
    }
    if c.script.contains('2') {
        return vec![c.script.replace('2', "3")];
    }
    let Some(last) = c.parts.last() else {
        return Vec::new();
    };
    let help = format!("{last} --help");
    if c.output.trim() == format!("No manual entry for {last}") {
        return vec![help];
    }
    let with_section = |section: &str| {
        let mut parts = c.parts.clone();
        parts.insert(1, section.to_string());
        parts.join(" ")
    };
    vec![with_section("3"), with_section("2"), help]
}

/// What hg suggests: a "did you mean one of" line, or a list of the commands
/// the word could stand for.
fn hg_possibilities(output: &str) -> Vec<String> {
    static DID_YOU_MEAN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"\n\(did you mean one of ([^?]+)\?\)").unwrap());
    if let Some(found) = DID_YOU_MEAN.captures(output) {
        return found[1].split(", ").map(String::from).collect();
    }
    match output.find("\n    ") {
        Some(at) => output[at + 5..]
            .trim_end()
            .split(' ')
            .filter(|word| !word.is_empty())
            .map(String::from)
            .collect(),
        None => Vec::new(),
    }
}

fn mercurial(c: &Command) -> Vec<String> {
    let Some(word) = args(c).first() else {
        return Vec::new();
    };
    closest(word, &hg_possibilities(&c.output), 0.6, true)
        .map(|new| {
            let mut parts = Vec::from(skip_sudo(c));
            parts[1] = new;
            format!("{}{}", sudo_prefix(c), crate::command::join(parts))
        })
        .into_iter()
        .collect()
}

// ---------------------------------------------------------------------------
// open

fn is_arg_url(c: &Command) -> bool {
    const MARKERS: [&str; 10] = [
        ".com", ".edu", ".info", ".io", ".ly", ".me", ".net", ".org", ".se", "www.",
    ];
    !c.script.contains("://") && MARKERS.iter().any(|marker| c.script.contains(marker))
}

fn file_does_not_exist(output: &str) -> bool {
    let output = output.trim();
    output.starts_with("The file ") && output.ends_with(" does not exist.")
}

fn open(c: &Command) -> Vec<String> {
    if is_arg_url(c) {
        return vec![c.script.replacen("open ", "open http://", 1)];
    }
    if file_does_not_exist(&c.output)
        && let Some((_, target)) = c.script.split_once(' ')
    {
        return ["touch", "mkdir"]
            .iter()
            .map(|make| format!("{make} {target} && {}", c.script))
            .collect();
    }
    Vec::new()
}

// ---------------------------------------------------------------------------
// port_already_in_use, rm_root, scm_correction, ssh_known_hosts

/// How programs say a port is taken; each captures the port.
static PORT_IN_USE: LazyLock<Vec<Regex>> = LazyLock::new(|| {
    [
        r"bind on address \('.*', (\d+)\)",
        r"Unable to bind [^ ]*:(\d+)",
        r"can't listen on port (\d+)",
        r"listen EADDRINUSE [^ ]*:(\d+)",
    ]
    .iter()
    .map(|pattern| Regex::new(pattern).unwrap())
    .collect()
});

fn used_port(output: &str) -> Option<String> {
    PORT_IN_USE
        .iter()
        .find_map(|pattern| pattern.captures(output))
        .map(|found| found[1].to_string())
}

/// The process id in the first row of an `lsof` listing.
fn first_pid(lsof: &str) -> Option<String> {
    lsof.lines()
        .nth(1)
        .and_then(|row| row.split_whitespace().nth(1))
        .map(String::from)
}

/// The process listening on the port. Only listeners, so a client that is
/// connected to the port is never the one offered.
fn listener_pid(port: &str) -> Option<String> {
    first_pid(&listing(
        "lsof",
        &["-nP", &format!("-iTCP:{port}"), "-sTCP:LISTEN"],
    )?)
}

fn is_recursive(flag: &str) -> bool {
    flag == "--recurse"
        || flag == "--recursive"
        || (!flag.starts_with("--") && flag.starts_with('-') && flag.contains(['r', 'R']))
}

/// `rm` on `/` that GNU rm refuses. rm is never run again, so with no output
/// the refusal is assumed when `rm` is recursive and GNU's.
fn rm_root(c: &Command, gnu: bool) -> bool {
    let parts = skip_sudo(c);
    parts.iter().any(|part| part == "rm")
        && parts.iter().any(|part| part == "/")
        && !c.script.contains("--no-preserve-root")
        && (c.output.contains("--no-preserve-root")
            || (c.output.trim().is_empty() && gnu && parts.iter().any(|part| is_recursive(part))))
}

/// `git` or `hg`, for the repository in the current folder.
fn actual_scm() -> Option<&'static str> {
    [(".git", "git"), (".hg", "hg")]
        .into_iter()
        .find(|(folder, _)| Path::new(folder).is_dir())
        .map(|(_, scm)| scm)
}

/// The version control to use instead, when the command used the other one.
fn wrong_scm(c: &Command, actual: impl Fn() -> Option<&'static str>) -> Option<&'static str> {
    let used = match c.parts.first().map_or("", String::as_str) {
        "git" if c.output_lower().contains("fatal: not a git repository") => "git",
        "hg" if c.output.contains("abort: no repository found") => "hg",
        _ => return None,
    };
    actual().filter(|scm| *scm != used)
}

fn close_substitutions(parts: &[String]) -> String {
    crate::command::join(parts.iter().map(|part| {
        if (part.starts_with("s/") || part.starts_with("-es/")) && !part.ends_with('/') {
            format!("{part}/")
        } else {
            part.clone()
        }
    }))
}

fn host_key_changed(output: &str) -> bool {
    static KEY_DIFFERS: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"Warning: the \S+ host key for '([^']+)' differs from the key for the IP address '([^']+)'")
            .unwrap()
    });
    output.contains("WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!")
        || output.contains("WARNING: POSSIBLE DNS SPOOFING DETECTED!")
        || KEY_DIFFERS.is_match(output)
}

/// ssh and scp names of a host: the part of `user@host` or `host:path` that
/// names the machine.
fn host_of(word: &str) -> &str {
    let host = word.rsplit_once('@').map_or(word, |(_, host)| host);
    host.split(':').next().unwrap_or(host)
}

fn script_host(c: &Command) -> Option<String> {
    // options that take a value, so the value is not mistaken for the host
    const WITH_VALUE: [&str; 20] = [
        "-B", "-b", "-c", "-D", "-E", "-e", "-F", "-I", "-i", "-J", "-L", "-l", "-m", "-O", "-o",
        "-P", "-p", "-Q", "-R", "-S",
    ];
    let mut words = args(c).iter();
    while let Some(word) = words.next() {
        if WITH_VALUE.contains(&word.as_str()) {
            words.next();
        } else if !word.starts_with('-') && (is_app(c, "ssh") || word.contains(':')) {
            return Some(host_of(word).to_string()).filter(|host| !host.is_empty());
        }
    }
    None
}

/// `ssh-keygen -R host`, shown in front of the command that failed. The file
/// and host come from what ssh printed when it can, since that is exactly
/// what is in `known_hosts`.
fn ssh_known_hosts(c: &Command) -> Vec<String> {
    static REMOVE_HINT: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"ssh-keygen -f "([^"]+)" -R "([^"]+)""#).unwrap());
    static OFFENDING: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?:Offending (?:key for IP|\S+ key)|Matching host key) in ([^:\s]+):\d+")
            .unwrap()
    });
    static KEY_FOR: LazyLock<Regex> = LazyLock::new(|| {
        Regex::new(r"(?i)host key for '?([^'\s]+)'? (?:has changed|differs)").unwrap()
    });
    let (file, host) = if let Some(hint) = REMOVE_HINT.captures(&c.output) {
        (Some(hint[1].to_string()), Some(hint[2].to_string()))
    } else {
        (
            OFFENDING
                .captures(&c.output)
                .map(|found| found[1].to_string()),
            KEY_FOR
                .captures(&c.output)
                .map(|found| found[1].to_string())
                .or_else(|| script_host(c)),
        )
    };
    let Some(host) = host else {
        return Vec::new();
    };
    let file = file
        .map(|file| format!(" -f {}", crate::command::quote(&file)))
        .unwrap_or_default();
    vec![format!(
        "ssh-keygen{file} -R {} && {}",
        crate::command::quote(&host),
        c.script
    )]
}

// ---------------------------------------------------------------------------
// switch_lang

/// What a US keyboard types where each layout's keys sit, in the same order.
const TARGET_LAYOUT: &str = "qwertyuiop[]asdfghjkl;'zxcvbnm,./QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?";

/// Layouts a command might have been typed in by mistake, as characters in the
/// same order as `TARGET_LAYOUT`. Greek also has a table of its own below,
/// because its accented letters do not line up one to one.
const SOURCE_LAYOUTS: [&str; 6] = [
    "йцукенгшщзхъфывапролджэячсмитьбю.ЙЦУКЕНГШЩЗХЪФЫВАПРОЛДЖЭЯЧСМИТЬБЮ,",
    "йцукенгшщзхїфівапролджєячсмитьбю.ЙЦУКЕНГШЩЗХЇФІВАПРОЛДЖЄЯЧСМИТЬБЮ,",
    "ضصثقفغعهخحجچشسیبلاتنمکگظطزرذدپو./ًٌٍَُِّْ][}{ؤئيإأآة»«:؛كٓژٰ‌ٔء><؟",
    "/'קראטוןםפ][שדגכעיחלךף,זסבהנמצתץ.QWERTYUIOP{}ASDFGHJKL:\"ZXCVBNM<>?",
    ";ςερτυθιοπ[]ασδφγηξκλ΄ζχψωβνμ,./:΅ΕΡΤΥΘΙΟΠ{}ΑΣΔΦΓΗΞΚΛ¨\"ΖΧΨΩΒΝΜ<>?",
    "ㅂㅈㄷㄱㅅㅛㅕㅑㅐㅔ[]ㅁㄴㅇㄹㅎㅗㅓㅏㅣ;'ㅋㅌㅊㅍㅠㅜㅡ,./ㅃㅉㄸㄲㅆㅛㅕㅑㅒㅖ{}ㅁㄴㅇㄹㅎㅗㅓㅏㅣ:\"ㅋㅌㅊㅍㅠㅜㅡ<>?",
];

/// Where Greek sits in `SOURCE_LAYOUTS`.
const GREEK: usize = 4;

const GREEK_TO_US: [(char, char); 79] = [
    (';', 'q'),
    ('ς', 'w'),
    ('ε', 'e'),
    ('ρ', 'r'),
    ('τ', 't'),
    ('υ', 'y'),
    ('θ', 'u'),
    ('ι', 'i'),
    ('ο', 'o'),
    ('π', 'p'),
    ('[', '['),
    (']', ']'),
    ('α', 'a'),
    ('σ', 's'),
    ('δ', 'd'),
    ('φ', 'f'),
    ('γ', 'g'),
    ('η', 'h'),
    ('ξ', 'j'),
    ('κ', 'k'),
    ('λ', 'l'),
    ('΄', '\''),
    ('ζ', 'z'),
    ('χ', 'x'),
    ('ψ', 'c'),
    ('ω', 'v'),
    ('β', 'b'),
    ('ν', 'n'),
    ('μ', 'm'),
    (',', ','),
    ('.', '.'),
    ('/', '/'),
    (':', 'Q'),
    ('΅', 'W'),
    ('Ε', 'E'),
    ('Ρ', 'R'),
    ('Τ', 'T'),
    ('Υ', 'Y'),
    ('Θ', 'U'),
    ('Ι', 'I'),
    ('Ο', 'O'),
    ('Π', 'P'),
    ('{', '{'),
    ('}', '}'),
    ('Α', 'A'),
    ('Σ', 'S'),
    ('Δ', 'D'),
    ('Φ', 'F'),
    ('Γ', 'G'),
    ('Η', 'H'),
    ('Ξ', 'J'),
    ('Κ', 'K'),
    ('Λ', 'L'),
    ('¨', ':'),
    ('"', '"'),
    ('Ζ', 'Z'),
    ('Χ', 'X'),
    ('Ψ', 'C'),
    ('Ω', 'V'),
    ('Β', 'B'),
    ('Ν', 'N'),
    ('Μ', 'M'),
    ('<', '<'),
    ('>', '>'),
    ('?', '?'),
    ('ά', 'a'),
    ('έ', 'e'),
    ('ύ', 'y'),
    ('ί', 'i'),
    ('ό', 'o'),
    ('ή', 'h'),
    ('ώ', 'v'),
    ('Ά', 'A'),
    ('Έ', 'E'),
    ('Ύ', 'Y'),
    ('Ί', 'I'),
    ('Ό', 'O'),
    ('Ή', 'H'),
    ('Ώ', 'V'),
];

const HEAD_LIST: [char; 19] = [
    'ㄱ', 'ㄲ', 'ㄴ', 'ㄷ', 'ㄸ', 'ㄹ', 'ㅁ', 'ㅂ', 'ㅃ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅉ', 'ㅊ', 'ㅋ',
    'ㅌ', 'ㅍ', 'ㅎ',
];
const BODY_LIST: [char; 21] = [
    'ㅏ', 'ㅐ', 'ㅑ', 'ㅒ', 'ㅓ', 'ㅔ', 'ㅕ', 'ㅖ', 'ㅗ', 'ㅘ', 'ㅙ', 'ㅚ', 'ㅛ', 'ㅜ', 'ㅝ', 'ㅞ',
    'ㅟ', 'ㅠ', 'ㅡ', 'ㅢ', 'ㅣ',
];
/// The first entry is a space: a syllable with no final consonant.
const TAIL_LIST: [char; 28] = [
    ' ', 'ㄱ', 'ㄲ', 'ㄳ', 'ㄴ', 'ㄵ', 'ㄶ', 'ㄷ', 'ㄹ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㄿ', 'ㅀ',
    'ㅁ', 'ㅂ', 'ㅄ', 'ㅅ', 'ㅆ', 'ㅇ', 'ㅈ', 'ㅊ', 'ㅋ', 'ㅌ', 'ㅍ', 'ㅎ',
];
const DOUBLE_LIST: [char; 17] = [
    'ㅘ', 'ㅙ', 'ㅚ', 'ㅝ', 'ㅞ', 'ㅟ', 'ㅢ', 'ㄳ', 'ㄵ', 'ㄶ', 'ㄺ', 'ㄻ', 'ㄼ', 'ㄽ', 'ㄾ', 'ㅀ',
    'ㅄ',
];
const DOUBLE_MOD_LIST: [&str; 17] = [
    "ㅗㅏ", "ㅗㅐ", "ㅗㅣ", "ㅜㅓ", "ㅜㅔ", "ㅜㅣ", "ㅡㅣ", "ㄱㅅ", "ㄴㅈ", "ㄴㅎ", "ㄹㄱ", "ㄹㅁ",
    "ㄹㅂ", "ㄹㅅ", "ㄹㅌ", "ㄹㅎ", "ㅂㅅ",
];

fn is_hangul(ch: char) -> bool {
    ('ㄱ'..='ㅎ').contains(&ch) || ('ㅏ'..='ㅣ').contains(&ch) || ('가'..='힣').contains(&ch)
}

/// The names the user's own alias for welp might have, so that a command
/// that turns into one is left alone.
fn alias_names() -> Vec<String> {
    let mut names = vec!["welp".to_string(), "fuck".to_string()];
    if let Ok(alias) = std::env::var("WELP_ALIAS") {
        names.push(alias);
    }
    names
}

/// The layout every word of the script could have been typed in.
fn typed_in(script: &str) -> Option<usize> {
    SOURCE_LAYOUTS.iter().position(|layout| {
        script.split(' ').all(|word| {
            word.chars()
                .all(|ch| layout.contains(ch) || ch == '-' || ch == '_')
        })
    })
}

fn switch_to_us(script: &str, layout: usize) -> String {
    if layout == GREEK {
        return script
            .chars()
            .map(|ch| {
                GREEK_TO_US
                    .iter()
                    .find(|(greek, _)| *greek == ch)
                    .map_or(ch, |(_, us)| *us)
            })
            .collect();
    }
    let target: Vec<char> = TARGET_LAYOUT.chars().collect();
    script
        .chars()
        .map(|ch| {
            SOURCE_LAYOUTS[layout]
                .chars()
                .position(|key| key == ch)
                .and_then(|at| target.get(at).copied())
                .unwrap_or(ch)
        })
        .collect()
}

/// Korean typed on a US keyboard comes out as syllables; this takes them
/// back apart into the letters the keys stand for.
fn decompose_hangul(script: &str) -> String {
    let double = |ch: char| -> String {
        match DOUBLE_LIST.iter().position(|double| *double == ch) {
            Some(at) => DOUBLE_MOD_LIST[at].to_string(),
            None => ch.to_string(),
        }
    };
    let mut letters = String::new();
    for ch in script.chars() {
        if ('가'..='힣').contains(&ch) {
            let offset = ch as u32 - '가' as u32;
            let head = (offset / 588) as usize;
            let body = ((offset % 588) / 28) as usize;
            let tail = (offset % 28) as usize;
            for letter in [HEAD_LIST[head], BODY_LIST[body], TAIL_LIST[tail]] {
                if letter != ' ' {
                    letters.push_str(&double(letter));
                }
            }
        } else {
            letters.push_str(&double(ch));
        }
    }
    letters
}

/// What the script says on a US keyboard, when it was typed in another
/// layout and that is not just the name of welp's own alias.
fn switched_layout(script: &str, aliases: &[String]) -> Option<String> {
    let script = if script.chars().any(is_hangul) {
        decompose_hangul(script)
    } else {
        script.to_string()
    };
    let layout = typed_in(&script)?;
    let switched = switch_to_us(&script, layout);
    (!aliases.contains(&switched)).then_some(switched)
}

// ---------------------------------------------------------------------------
// the rest

static PROMPT_LITERAL: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"^\s*\$ \S+").unwrap());

static TMUX_AMBIGUOUS: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^ambiguous command: (.*), could be: (.*)").unwrap());

static UNKNOWN_COMMAND: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"([^:]*): Unknown command").unwrap());

static DID_YOU_MEAN_UNTIL_QUESTION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"Did you mean ([^?]*)").unwrap());

/// `systemctl nginx start`: two words after systemctl, and systemctl does
/// not know the first as an operation.
fn misordered_systemctl(c: &Command) -> bool {
    c.output.contains("Unknown operation '") && args(c).len() == 2
}

/// The part of an address before the first slash that names the machine,
/// whether or not it comes with `https://`.
fn netloc(url: &str) -> &str {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    rest.split(['/', '?', '#']).next().unwrap_or("")
}

fn whois(c: &Command) -> Vec<String> {
    let Some(url) = args(c).first() else {
        return Vec::new();
    };
    if c.script.contains('/') {
        let host = netloc(url);
        return if host.is_empty() {
            Vec::new()
        } else {
            vec![format!("whois {host}")]
        };
    }
    if !c.script.contains('.') {
        return Vec::new();
    }
    let labels: Vec<&str> = url.split('.').collect();
    (1..labels.len())
        .map(|from| format!("whois {}", labels[from..].join(".")))
        .collect()
}

/// `git-log`: the word before the first hyphen is a program, the whole word
/// is not.
fn wrong_hyphen(c: &Command, executable: impl Fn(&str) -> bool) -> bool {
    let first = c.program();
    match first.split_once('-') {
        Some((program, _)) => !executable(first) && executable(program),
        None => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{all, rule_matches};

    /// The fixes one rule gives for a command, whether or not other rules or
    /// its being off by default would get in the way.
    fn fixes(name: &str, script: &str, output: &str) -> Vec<String> {
        fixes_of(name, &Command::new(script, output))
    }

    fn fixes_of(name: &str, command: &Command) -> Vec<String> {
        let rule = all()
            .into_iter()
            .find(|rule| rule.name == name)
            .unwrap_or_else(|| panic!("no rule named {name}"));
        if (rule.matches)(command) {
            (rule.fixes)(command)
        } else {
            Vec::new()
        }
    }

    #[test]
    fn difflib_scores_match_python() {
        // difflib.SequenceMatcher(None, "abcd", "bcde").ratio() == 0.75
        assert_eq!(ratio("abcd", "bcde"), 0.75);
        assert_eq!(ratio("", ""), 1.0);
        assert_eq!(ratio("ls cat", "ls cet"), 2.0 * 5.0 / 12.0);
        // a tie goes to the later string
        assert_eq!(
            close_matches("ab", &["ax", "ay", "az"], 2, 0.1),
            ["az", "ay"]
        );
    }

    #[test]
    fn cd_cs_types_cd() {
        assert_eq!(
            fixes("cd_cs", "cs /etc/", "cs: command not found"),
            ["cd /etc/"]
        );
        assert_eq!(fixes("cd_cs", "cs", "cs: command not found"), ["cd"]);
        assert!(!rule_matches("cd_cs", "csh -x", "csh: command not found"));
    }

    #[test]
    fn cp_creates_the_destination() {
        for (script, output, fix) in [
            (
                "cp foo bar/",
                "cp: directory foo does not exist\n",
                "mkdir -p bar/ && cp foo bar/",
            ),
            (
                "mv foo bar/",
                "No such file or directory",
                "mkdir -p bar/ && mv foo bar/",
            ),
            (
                "cp foo bar/baz/",
                "cp: directory foo does not exist\n",
                "mkdir -p bar/baz/ && cp foo bar/baz/",
            ),
        ] {
            assert_eq!(fixes("cp_create_destination", script, output), [fix]);
        }
        // not cp or mv, or nothing wrong
        assert!(!rule_matches(
            "cp_create_destination",
            "ls foo",
            "No such file or directory"
        ));
        assert!(!rule_matches(
            "cp_create_destination",
            "cp foo bar",
            "cp: permission denied"
        ));
    }

    #[test]
    fn mv_looks_at_the_files_because_it_prints_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let base = tmp.path().to_str().unwrap();
        std::fs::write(tmp.path().join("foo"), "").unwrap();
        std::fs::create_dir(tmp.path().join("there")).unwrap();

        // the folder is missing: both rules have a fix
        let script = format!("mv {base}/foo {base}/missing/");
        assert_eq!(
            fixes("cp_create_destination", &script, ""),
            [format!("mkdir -p {base}/missing/ && {script}")]
        );
        assert_eq!(
            fixes("no_such_file", &script, ""),
            [format!("mkdir -p {base}/missing && {script}")]
        );
        let script = format!("mv {base}/foo {base}/missing/new");
        assert_eq!(
            fixes("no_such_file", &script, ""),
            [format!("mkdir -p {base}/missing && {script}")]
        );
        // a plain rename, a folder that is there, and a source that is not
        for script in [
            format!("mv {base}/foo {base}/renamed"),
            format!("mv {base}/foo {base}/there/"),
            format!("mv {base}/nope {base}/missing/"),
        ] {
            assert!(
                !rule_matches("cp_create_destination", &script, ""),
                "{script}"
            );
            assert!(!rule_matches("no_such_file", &script, ""), "{script}");
        }
    }

    #[test]
    fn untar_goes_into_its_own_folder() {
        for ext in TAR_EXTENSIONS {
            for (script, fixed) in [
                (
                    format!("tar xvf foo{ext}"),
                    format!("mkdir -p foo && tar xvf foo{ext} -C foo"),
                ),
                (
                    format!("tar -xvf foo{ext}"),
                    format!("mkdir -p foo && tar -xvf foo{ext} -C foo"),
                ),
                (
                    format!("tar --extract -f foo{ext}"),
                    format!("mkdir -p foo && tar --extract -f foo{ext} -C foo"),
                ),
                (
                    format!("tar xvf \"foo bar{ext}\""),
                    format!("mkdir -p 'foo bar' && tar xvf \"foo bar{ext}\" -C 'foo bar'"),
                ),
            ] {
                assert_eq!(fixes("dirty_untar", &script, ""), [fixed]);
            }
        }
        for script in [
            "tar xvf foo.tar -C out",
            "tar czf foo.tar.gz dir",
            "tar --exclude=x -czf foo.tar.gz dir",
            "tar xvf foo.txt",
            "tar tf foo.tar",
        ] {
            assert!(!rule_matches("dirty_untar", script, ""), "{script}");
        }
    }

    #[test]
    fn unzip_goes_into_its_own_folder_when_it_holds_many_files() {
        // an end-of-archive record that says how many files there are
        let end_record = |files: u8| {
            let mut record = b"PK\x05\x06".to_vec();
            record.extend([0, 0, 0, 0, files, 0, files, 0]);
            record.extend([0u8; 12]);
            record
        };
        assert_eq!(zip_entry_count(&end_record(4)), Some(4));
        assert_eq!(zip_entry_count(b"not a zip"), None);

        let many = |_: &str| Some(4);
        let one = |_: &str| Some(1);
        let unreadable = |_: &str| None;
        for (script, fixed) in [
            ("unzip café", "unzip café -d café"),
            ("unzip foo", "unzip foo -d foo"),
            ("unzip 'foo bar.zip'", "unzip 'foo bar.zip' -d 'foo bar'"),
            ("unzip foo.zip", "unzip foo.zip -d foo"),
        ] {
            let command = Command::new(script, "");
            assert_eq!(
                unzip_folder(&command, many).map(|dir| format!("{script} -d {dir}")),
                Some(fixed.to_string())
            );
            assert_eq!(unzip_folder(&command, one), None);
            assert_eq!(unzip_folder(&command, unreadable), None);
        }
        assert_eq!(
            unzip_folder(&Command::new("unzip -d out foo.zip", ""), many),
            None
        );
        assert!(!rule_matches("dirty_unzip", "unzip welp-missing.zip", ""));
    }

    #[test]
    fn unzip_reads_a_real_archive() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("two.zip");
        let mut archive = b"PK\x05\x06".to_vec();
        archive.extend([0, 0, 0, 0, 2, 0, 2, 0]);
        archive.extend([0u8; 12]);
        std::fs::write(&path, archive).unwrap();
        let path = path.to_str().unwrap();
        assert_eq!(zip_entries(path), Some(2));
        assert_eq!(
            fixes("dirty_unzip", &format!("unzip {path}"), ""),
            [format!(
                "unzip {path} -d {}",
                crate::command::quote(path.strip_suffix(".zip").unwrap())
            )]
        );
    }

    #[test]
    fn dry_drops_the_repeated_word() {
        assert_eq!(fixes("dry", "cd cd foo", ""), ["cd foo"]);
        assert_eq!(
            fixes("dry", "git git push origin/master", ""),
            ["git push origin/master"]
        );
        assert!(!rule_matches("dry", "git push", ""));
        assert!(!rule_matches("dry", "ls", ""));
    }

    #[test]
    fn fix_alt_space_swaps_the_space() {
        let script = "ps -ef |\u{a0}grep foo";
        assert_eq!(
            fixes(
                "fix_alt_space",
                script,
                "-bash:\u{a0}grep: command not found"
            ),
            ["ps -ef | grep foo"]
        );
        assert!(!rule_matches("fix_alt_space", script, ""));
        assert!(!rule_matches(
            "fix_alt_space",
            "ps -ef | grep foo",
            "grep: command not found"
        ));
    }

    #[test]
    fn fix_file_finds_the_line() {
        let anywhere = |_: &str| true;
        let nowhere = |_: &str| false;
        for (output, file, line) in [
            (
                "\na.c: In function 'main':\na.c:3:1: error: expected expression before '}' token\n }\n  ^\n",
                "a.c",
                "3",
            ),
            ("\na.c:3:1: error: expected expression\n}\n^\n", "a.c", "3"),
            (
                "\nsyntax error at a.pl line 3, at EOF\nExecution of a.pl aborted due to compilation errors.\n",
                "a.pl",
                "3",
            ),
            (
                "\nSearch pattern not terminated at a.pl line 2.\n",
                "a.pl",
                "2",
            ),
            ("\na.sh: line 2: foo: command not found\n", "a.sh", "2"),
            ("\na.sh:2: command not found: foo\n", "a.sh", "2"),
            (
                "\na.rs:2:5: 2:6 error: unexpected token: `+`\na.rs:2     +\n           ^\n",
                "a.rs",
                "2",
            ),
            (
                "\n   Compiling test v0.1.0 (file:///tmp/fix-error/test)\n   src/lib.rs:3:5: 3:6 error: unexpected token: `+`\n   src/lib.rs:3     +\n                    ^\nCould not compile `test`.\nTo learn more, run the command again with --verbose.\n",
                "src/lib.rs",
                "3",
            ),
            (
                "\n  File \"a.py\", line 2\n      +\n          ^\nSyntaxError: invalid syntax\n",
                "a.py",
                "2",
            ),
            (
                "\nTraceback (most recent call last):\n  File \"a.py\", line 8, in <module>\n    match(\"foo\")\n  File \"a.py\", line 5, in match\n    m = re.search(None, command)\n  File \"/usr/lib/python3.4/re.py\", line 170, in search\n    return _compile(pattern, flags).search(string)\nTypeError: first argument must be string or compiled pattern\n",
                "a.py",
                "8",
            ),
            (
                "\nTraceback (most recent call last):\n  File \"café.py\", line 8, in <module>\n    match(\"foo\")\nTypeError: first argument must be string or compiled pattern\n",
                "café.py",
                "8",
            ),
            (
                "\na.rb:3: syntax error, unexpected keyword_end\n",
                "a.rb",
                "3",
            ),
            ("\nlua: a.lua:2: unexpected symbol near '+'\n", "a.lua", "2"),
            (
                "\nfish: Unknown command 'foo'\n/tmp/fix-error/a.sh (line 2): foo\n                              ^\n",
                "/tmp/fix-error/a.sh",
                "2",
            ),
            (
                "\nawk: ./a:2: BEGIN { print \"Hello, world!\" + }\nawk: ./a:2:                                 ^ syntax error\n",
                "./a",
                "2",
            ),
            (
                "\nllc: a.ll:1:2: error: expected top-level entity\n+\n^\n",
                "a.ll",
                "1",
            ),
            (
                "\ncan't load package:\na.go:1:2: expected 'package', found '+'\n",
                "a.go",
                "1",
            ),
            (
                "\nbidule\nmake: bidule: Command not found\nMakefile:2: recipe for target 'target' failed\nmake: *** [target] Error 127\n",
                "Makefile",
                "2",
            ),
            (
                "\nfatal: bad config file line 1 in /home/martin/.config/git/config\n",
                "/home/martin/.config/git/config",
                "1",
            ),
            (
                "\n/Users/pablo/Workspace/barebones/fuck.js:2\nconole.log(arg);  // this should read console.log(arg);\n^\nReferenceError: conole is not defined\n    at /Users/pablo/Workspace/barebones/fuck.js:2:5\n    at Array.forEach (native)\n    at Module._compile (module.js:460:26)\n",
                "/Users/pablo/Workspace/barebones/fuck.js",
                "2",
            ),
            (
                "\n./tests/rules/test_systemctl.py:17:80: E501 line too long (93 > 79 characters)\n./tests/rules/test_systemctl.py:18:80: E501 line too long (103 > 79 characters)\n",
                "./tests/rules/test_systemctl.py",
                "17",
            ),
        ] {
            assert_eq!(
                error_location(output, anywhere),
                Some((file.to_string(), line.to_string())),
                "{output}"
            );
            // only a file that is there counts
            assert_eq!(error_location(output, nowhere), None, "{output}");
        }
        assert_eq!(error_location("", anywhere), None);
        assert_eq!(error_location("all good\n", anywhere), None);
    }

    #[test]
    fn fix_file_opens_the_editor_on_a_real_file() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("a.sh");
        std::fs::write(&file, "foo\n").unwrap();
        let file = file.to_str().unwrap();
        let output = format!("{file}: line 2: foo: command not found\n");
        assert_eq!(
            error_location(&output, is_file),
            Some((file.to_string(), "2".to_string()))
        );
        assert_eq!(
            error_location("a-file-welp-does-not-have: line 2: foo\n", is_file),
            None
        );
        assert_eq!(
            open_at("vim", file, "2", "sh a.sh"),
            format!("vim {file} +2 && sh a.sh")
        );
        assert_eq!(
            open_at("vim", "my file.sh", "2", "sh 'my file.sh'"),
            "vim 'my file.sh' +2 && sh 'my file.sh'"
        );
        // nothing in the output to open
        assert!(!rule_matches("fix_file", "sh a.sh", ""));
        assert!(!rule_matches("fix_file", "sh a.sh", "all fine\n"));
    }

    #[test]
    fn grep_puts_the_file_last() {
        let tmp = tempfile::tempdir().unwrap();
        let file = tmp.path().join("test");
        std::fs::write(&file, "").unwrap();
        let file = file.to_str().unwrap();
        let output = format!("grep: {file}: No such file or directory");

        assert_eq!(
            fixes(
                "grep_arguments_order",
                &format!("grep {file} test.py"),
                &output
            ),
            [format!("grep test.py {file}")]
        );
        assert_eq!(
            fixes(
                "grep_arguments_order",
                &format!("egrep -lir . {file}"),
                &output
            ),
            [format!("egrep -lir {file} .")]
        );
        assert_eq!(
            fixes(
                "grep_arguments_order",
                &format!("grep . {file} -lir"),
                &output
            ),
            [format!("grep {file} -lir .")]
        );
        // nothing wrong, not grep, or no file to move
        assert!(!rule_matches(
            "grep_arguments_order",
            &format!("grep test {file}"),
            ""
        ));
        assert!(!rule_matches(
            "grep_arguments_order",
            &format!("cat {file}"),
            &output
        ));
        assert!(!rule_matches(
            "grep_arguments_order",
            "grep welp-nope welp-nada",
            &output
        ));
    }

    #[test]
    fn history_offers_the_closest_command() {
        let command = |script: &str| {
            Command::new(script, "").with_history(vec!["ls cat".into(), "diff x".into()])
        };
        let known = |_: &str| true;
        for (script, fix) in [("ls cet", "ls cat"), ("daff x", "diff x")] {
            assert_eq!(history_matches(&command(script), known), [fix]);
        }
        for script in ["apt-get", "nocommand y"] {
            assert!(history_matches(&command(script), known).is_empty());
        }
        // only commands that start with a program that exists
        assert!(history_matches(&command("ls cet"), |program| program != "ls").is_empty());
        // the command being fixed is not its own fix
        let same = Command::new("ls cat", "").with_history(vec!["ls cat".into()]);
        assert!(history_matches(&same, known).is_empty());
        // and no history, no fix
        assert!(!rule_matches("history", "ls cet", ""));
    }

    #[test]
    fn hostscli_lists_websites_or_fixes_the_command() {
        let no_website = "hostscli.errors.WebsiteImportError:\nNo Domain list found for website: a_website_that_does_not_exist\nPlease raise a Issue here: https://github.com/dhilipsiva/hostscli/issues/new\nif you think we should add domains for this website.\ntype `hostscli websites` to see a list of websites that you can block/unblock\n";
        assert_eq!(
            fixes(
                "hostscli",
                "hostscli block a_website_that_does_not_exist",
                no_website
            ),
            ["hostscli websites"]
        );
        assert_eq!(
            fixes(
                "hostscli",
                "hostscli blok facebook",
                "Error: No such command \"blok\"."
            ),
            [
                "hostscli block facebook",
                "hostscli unblock facebook",
                "hostscli block_all facebook"
            ]
        );
        assert!(!rule_matches("hostscli", "hostscli block facebook", ""));
        assert!(!rule_matches("hostscli", "ls", no_website));
    }

    #[test]
    fn ifconfig_uses_an_interface_that_is_there() {
        let output = "wlan0: error fetching interface information: Device not found";
        let listing = "\nwlp2s0    Link encap:Ethernet  HWaddr 5c:51:4f:7c:58:5d\n          inet addr:192.168.0.103  Bcast:192.168.0.255  Mask:255.255.255.0\n          UP BROADCAST RUNNING MULTICAST  MTU:1500  Metric:1\n          RX packets:23581604 errors:0 dropped:0 overruns:0 frame:0\n";
        assert_eq!(interfaces(listing), ["wlp2s0"]);
        assert_eq!(
            ifconfig_fixes(&Command::new("ifconfig wlan0", output), listing),
            ["ifconfig wlp2s0"]
        );
        assert_eq!(
            ifconfig_fixes(&Command::new("ifconfig -s wlan0", output), listing),
            ["ifconfig -s wlp2s0"]
        );
        // macOS names its interfaces with a colon and indents with tabs
        assert_eq!(
            interfaces(
                "lo0: flags=8049<UP>\n\tinet 127.0.0.1\nen0: flags=8863<UP>\n\tether 00:00\n"
            ),
            ["lo0", "en0"]
        );
        assert!(rule_matches(
            "ifconfig_device_not_found",
            "ifconfig wlan0",
            output
        ));
        assert!(!rule_matches(
            "ifconfig_device_not_found",
            "config wlan0",
            output
        ));
        assert!(!rule_matches(
            "ifconfig_device_not_found",
            "ifconfig eth0",
            ""
        ));
    }

    #[test]
    fn ln_s_puts_the_link_last() {
        let output = "ln: failed to create symbolic link 'source': File exists";
        let exists = |_: &str| true;
        for (script, fixed) in [
            ("ln -s dest source", "ln -s source dest"),
            ("ln dest -s source", "ln -s source dest"),
            ("ln dest source -s", "ln source -s dest"),
            ("sudo ln -s dest source", "sudo ln -s source dest"),
        ] {
            assert_eq!(
                ln_fix(&Command::new(script, output), exists).as_deref(),
                Some(fixed)
            );
        }
        for (script, output, exists_) in [
            ("ln dest source", output, true),
            ("ls -s dest source", output, true),
            ("ln -s dest source", "", true),
            ("ln -s dest source", output, false),
        ] {
            let command = Command::new(script, output);
            assert_eq!(ln_fix(&command, |_| exists_), None, "{script}");
        }
    }

    #[test]
    fn ln_s_rule_swaps_real_files() {
        let tmp = tempfile::tempdir().unwrap();
        let dest = tmp.path().join("dest");
        std::fs::write(&dest, "").unwrap();
        let dest = dest.to_str().unwrap();
        let output = "ln: failed to create symbolic link 'source': File exists";
        assert_eq!(
            fixes("ln_s_order", &format!("ln -s {dest} source"), output),
            [format!("ln -s source {dest}")]
        );
        assert!(!rule_matches(
            "ln_s_order",
            "ln -s welp-nope source",
            output
        ));
    }

    #[test]
    fn long_form_help_follows_the_hint() {
        assert_eq!(
            fixes(
                "long_form_help",
                "grep -h",
                "Try 'grep --help' for more information."
            ),
            ["grep --help"]
        );
        assert_eq!(
            fixes(
                "long_form_help",
                "tar -h",
                "tar: unknown option\nRun 'tar --help' or 'tar --usage' for details."
            ),
            ["tar --help"]
        );
        // no hint, but the output talks about --help
        assert_eq!(
            fixes(
                "long_form_help",
                "docker run -h",
                "see --help for the options"
            ),
            ["docker run --help"]
        );
        assert!(!rule_matches("long_form_help", "", ""));
        assert!(!rule_matches("long_form_help", "grep -h", "no such flag"));
    }

    #[test]
    fn ls_all_shows_hidden_files_when_ls_prints_nothing() {
        assert_eq!(fixes("ls_all", "ls empty_dir", ""), ["ls -A empty_dir"]);
        assert_eq!(fixes("ls_all", "ls", ""), ["ls -A"]);
        assert!(!rule_matches("ls_all", "ls", "file.py\n"));
        assert!(!rule_matches("ls_all", "cat", ""));
    }

    #[test]
    fn ls_lah_adds_flags() {
        assert_eq!(fixes("ls_lah", "ls file.py", ""), ["ls -lah file.py"]);
        assert_eq!(fixes("ls_lah", "ls", ""), ["ls -lah"]);
        assert_eq!(fixes("ls_lah", "ls /opt", ""), ["ls -lah /opt"]);
        assert!(!rule_matches("ls_lah", "ls -lah /opt", ""));
        // like thefuck's for_app, a sudo in front is not an ls
        assert!(!rule_matches("ls_lah", "sudo ls", "permission denied"));
        assert!(!rule_matches("ls_lah", "pacman -S binutils", ""));
        assert!(!rule_matches("ls_lah", "lsof", ""));
    }

    #[test]
    fn man_tries_other_sections() {
        assert_eq!(
            fixes("man", "man read", ""),
            ["man 3 read", "man 2 read", "read --help"]
        );
        assert_eq!(
            fixes("man", "man missing", "No manual entry for missing\n"),
            ["missing --help"]
        );
        for (script, fixed) in [
            ("man 2 read", "man 3 read"),
            ("man 3 read", "man 2 read"),
            ("man -s2 read", "man -s3 read"),
            ("man -s3 read", "man -s2 read"),
            ("man -s 2 read", "man -s 3 read"),
            ("man -s 3 read", "man -s 2 read"),
        ] {
            assert_eq!(fixes("man", script, ""), [fixed]);
        }
        assert!(!rule_matches("man", "man", ""));
        assert!(!rule_matches("man", "man ", ""));
        assert!(!rule_matches("man", "manage read", ""));
    }

    #[test]
    fn mercurial_uses_the_command_hg_suggests() {
        for (script, output, fixed) in [
            (
                "hg base",
                "hg: unknown command 'base'\n(did you mean one of blame, phase, rebase?)",
                "hg rebase",
            ),
            (
                "hg branchch",
                "hg: unknown command 'branchch'\n(did you mean one of branch, branches?)",
                "hg branch",
            ),
            (
                "hg vert",
                "hg: unknown command 'vert'\n(did you mean one of revert?)",
                "hg revert",
            ),
            (
                "hg lgo -r tip",
                "hg: command 're' is ambiguous:\n(did you mean one of log?)",
                "hg log -r tip",
            ),
            (
                "hg rerere",
                "hg: unknown command 'rerere'\n(did you mean one of revert?)",
                "hg revert",
            ),
            (
                "hg re",
                "hg: command 're' is ambiguous:\n    rebase recover remove rename resolve revert",
                "hg rebase",
            ),
            (
                "hg re re",
                "hg: command 're' is ambiguous:\n    rebase recover remove rename resolve revert\n",
                "hg rebase re",
            ),
        ] {
            assert_eq!(fixes("mercurial", script, output), [fixed]);
        }
        assert_eq!(
            hg_possibilities(
                "hg: command 're' is ambiguous:\n    rebase recover remove rename resolve revert"
            ),
            ["rebase", "recover", "remove", "rename", "resolve", "revert"]
        );
        for (script, output) in [
            ("hg", "\nMercurial Distributed SCM\n\nbasic commands:"),
            (
                "hg asdf",
                "hg: unknown command 'asdf'\nMercurial Distributed SCM\n\nbasic commands:",
            ),
            (
                "hg me",
                "\nabort: no repository found in './thefuck' (.hg not found)!",
            ),
            (
                "hg co",
                "\nabort: no repository found in './thefuck' (.hg not found)!",
            ),
        ] {
            assert!(!rule_matches("mercurial", script, output), "{script}");
        }
    }

    #[test]
    fn no_such_file_makes_the_folder() {
        assert_eq!(
            fixes(
                "no_such_file",
                "mv foo bar/foo",
                "mv: cannot move 'foo' to 'bar/foo': No such file or directory"
            ),
            ["mkdir -p bar && mv foo bar/foo"]
        );
        assert_eq!(
            fixes(
                "no_such_file",
                "mv foo bar/",
                "mv: cannot move 'foo' to 'bar/': No such file or directory"
            ),
            ["mkdir -p bar && mv foo bar/"]
        );
        assert_eq!(
            fixes(
                "no_such_file",
                "cp foo 'a b/foo'",
                "cp: cannot create regular file 'a b/foo': No such file or directory"
            ),
            ["mkdir -p 'a b' && cp foo 'a b/foo'"]
        );
        assert!(!rule_matches(
            "no_such_file",
            "mv foo bar/foo",
            "mv: permission denied"
        ));
        assert!(!rule_matches(
            "no_such_file",
            "ls foo",
            "ls: cannot access 'foo': No such file or directory"
        ));
    }

    #[test]
    fn open_adds_http_or_creates_the_file() {
        for program in ["open", "xdg-open", "gnome-open", "kde-open"] {
            let script = format!("{program} foo.io");
            let output = "The file foo.io does not exist.\n";
            assert_eq!(
                fixes("open", &script, output),
                [format!("{program} http://foo.io")]
            );
        }
        for site in [
            "foo.com",
            "foo.edu",
            "foo.info",
            "foo.io",
            "foo.ly",
            "foo.me",
            "foo.net",
            "foo.org",
            "foo.se",
            "www.foo.ru",
        ] {
            assert!(
                is_arg_url(&Command::new(&format!("open {site}"), "")),
                "{site}"
            );
        }
        for file in ["foo", "bar.txt", "egg.doc"] {
            assert!(
                !is_arg_url(&Command::new(&format!("open {file}"), "")),
                "{file}"
            );
        }
        assert_eq!(
            fixes("open", "open nonest", "The file nonest does not exist.\n"),
            ["touch nonest && open nonest", "mkdir nonest && open nonest"]
        );
        // an address that already has its scheme, and a file that opened
        assert!(!rule_matches("open", "open https://foo.com", ""));
        assert!(!rule_matches("open", "open notes.txt", ""));
        assert!(!rule_matches("open", "ls foo.com", ""));
    }

    #[test]
    fn path_from_history_swaps_in_a_longer_path() {
        let history = || vec!["cd /opt/java".to_string(), "ls ~/work/project/".to_string()];
        let known = |_: &str| true;
        let exists = |_: &str| true;
        for (script, output, fixed) in [
            (
                "ls project",
                "no such file or directory: project",
                "ls ~/work/project",
            ),
            ("cd project", "can't cd to project", "cd ~/work/project"),
            ("cd java", "can't cd to java", "cd /opt/java"),
            (
                "cd java",
                "sh: line 0: cd: java: No such file or directory",
                "cd /opt/java",
            ),
            (
                "ls java",
                "ls: java: No such file or directory",
                "ls /opt/java",
            ),
        ] {
            let command = Command::new(script, output).with_history(history());
            let missing = missing_path(&command).unwrap();
            assert_eq!(
                paths_from_history(&command, &missing, known, exists)[0],
                fixed,
                "{script}"
            );
        }
        // the path that shows up most comes first
        let command = Command::new("cd src", "can't cd to src").with_history(vec![
            "cd /a/src".into(),
            "ls /b/src".into(),
            "ls /b/src/".into(),
        ]);
        assert_eq!(
            paths_from_history(&command, "src", known, exists),
            ["cd /b/src", "cd /a/src"]
        );
        // only paths that exist, and only commands that start with a program
        assert!(paths_from_history(&command, "src", known, |_| false).is_empty());
        assert!(paths_from_history(&command, "src", |_| false, exists).is_empty());
        for (script, output) in [
            ("myapp cats", "no such file or directory: project"),
            ("cd project", ""),
        ] {
            assert!(
                !rule_matches("path_from_history", script, output),
                "{script}"
            );
        }
    }

    #[test]
    fn port_in_use_finds_the_port_and_the_listener() {
        let outputs = [
            "\nDE 70% 1/1 build modulesevents.js:141\n      throw er; // Unhandled 'error' event\n      ^\nError: listen EADDRINUSE 127.0.0.1:8080\n    at Object.exports._errnoException (util.js:873:11)\n",
            "\n[6:40:01 AM] <START> Building Dependency Graph\n ERROR  Packager can't listen on port 8080\nMost likely another process is already using this port\nRun the following command to find out which process:\n   lsof -n -i4TCP:8080\n",
            "\nOSError: [Errno 98] error while attempting to bind on address ('0.0.0.0', 8080): address already in use\nTask was destroyed but it is pending!\n",
            "Unable to bind 0.0.0.0:8080",
        ];
        for output in outputs {
            assert_eq!(used_port(output).as_deref(), Some("8080"), "{output}");
        }
        assert_eq!(used_port(""), None);
        assert_eq!(used_port("Everything is fine"), None);
        let lsof = "COMMAND   PID USER   FD   TYPE DEVICE SIZE/OFF NODE NAME\nnode    18233 nvbn   16u  IPv4 557134      0t0  TCP localhost:http-alt (LISTEN)\n";
        assert_eq!(first_pid(lsof).as_deref(), Some("18233"));
        // lsof printing nothing: nothing listens
        assert_eq!(first_pid(""), None);
        assert_eq!(first_pid("COMMAND PID USER\n"), None);
        assert!(!rule_matches("port_already_in_use", "./app", ""));
    }

    #[test]
    fn prove_looks_inside_folders() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().to_str().unwrap();
        let output = "Files=0, Tests=0,  0 wallclock secs ( 0.00 usr +  0.00 sys =  0.00 CPU)\nResult: NOTESTS";
        assert_eq!(
            fixes("prove_recursively", &format!("prove -lv {dir}"), output),
            [format!("prove -r -lv {dir}")]
        );
        assert_eq!(
            fixes("prove_recursively", &format!("prove {dir}"), output),
            [format!("prove -r {dir}")]
        );
        for script in [
            format!("prove -r {dir}"),
            format!("prove --recurse {dir}"),
            "prove -lv welp-nope".to_string(),
        ] {
            assert!(
                !rule_matches("prove_recursively", &script, output),
                "{script}"
            );
        }
        assert!(!rule_matches(
            "prove_recursively",
            &format!("prove {dir}"),
            "All tests successful."
        ));
    }

    #[test]
    fn dollar_prompt_is_dropped() {
        let output = "$: command not found";
        for (script, fixed) in [
            ("$ cd newdir", "cd newdir"),
            (" $ cd newdir", "cd newdir"),
            ("$ $ cd newdir", "cd newdir"),
            (" $ $ cd newdir", "cd newdir"),
            ("$ python3 -m virtualenv env", "python3 -m virtualenv env"),
            (
                " $ $ $ python3 -m virtualenv env",
                "python3 -m virtualenv env",
            ),
        ] {
            assert_eq!(
                fixes("remove_shell_prompt_literal", script, output),
                [fixed]
            );
        }
        for (script, output) in [
            ("$", "$: command not found"),
            (" $", "$: command not found"),
            ("$?", "127: command not found"),
            (" $?", "127: command not found"),
            ("", ""),
            ("$ cd newdir", ""),
        ] {
            assert!(
                !rule_matches("remove_shell_prompt_literal", script, output),
                "{script}"
            );
        }
    }

    #[test]
    fn trailing_cedilla_is_dropped() {
        assert_eq!(fixes("remove_trailing_cedilla", "wrongç", ""), ["wrong"]);
        assert_eq!(
            fixes("remove_trailing_cedilla", "wrong with argsç", ""),
            ["wrong with args"]
        );
        assert!(!rule_matches("remove_trailing_cedilla", "façade", ""));
    }

    #[test]
    fn rm_root_adds_the_flag() {
        let output = "rm: it is dangerous to operate recursively on '/'\nrm: use --no-preserve-root to override this failsafe\n";
        let command = |script: &str, output: &str| Command::new(script, output);
        assert!(rm_root(
            &command("rm -rf /", "add --no-preserve-root"),
            false
        ));
        assert!(rm_root(&command("sudo rm -rf /", output), false));
        assert_eq!(
            fixes("rm_root", "rm -rf /", "add --no-preserve-root"),
            ["rm -rf / --no-preserve-root"]
        );
        // rm is never run again, so on GNU systems a recursive rm of / is enough
        assert!(rm_root(&command("rm -rf /", ""), true));
        assert!(!rm_root(&command("rm -rf /", ""), false));
        assert!(!rm_root(&command("rm /", ""), true));
        assert!(!rm_root(&command("rm -rf /tmp/x", ""), true));
        // not rm, already there, or nothing wrong
        assert!(!rm_root(&command("ls", "add --no-preserve-root"), true));
        assert!(!rm_root(
            &command("rm --no-preserve-root /", "add --no-preserve-root"),
            true
        ));
        assert!(!rm_root(&command("rm -rf / --no-preserve-root", ""), true));
    }

    #[test]
    fn scm_correction_uses_the_repository_that_is_here() {
        let git_error = "fatal: Not a git repository (or any of the parent directories): .git";
        let hg_error = "abort: no repository found in '/home/nvbn/exp/thefuck' (.hg not found)!";
        let wrong = |script: &str, output: &str, actual: Option<&'static str>| {
            wrong_scm(&Command::new(script, output), || actual)
        };
        assert_eq!(wrong("git log", git_error, Some("hg")), Some("hg"));
        assert_eq!(wrong("hg log", hg_error, Some("git")), Some("git"));
        assert_eq!(
            wrong(
                "git log",
                "fatal: not a git repository (or any of the parent directories): .git",
                Some("hg")
            ),
            Some("hg")
        );
        assert_eq!(wrong("git log", "", Some("hg")), None);
        assert_eq!(wrong("git log", git_error, None), None);
        assert_eq!(wrong("hg log", hg_error, None), None);
        assert_eq!(wrong("not-scm log", hg_error, Some("git")), None);
        // already the right one
        assert_eq!(wrong("git log", git_error, Some("git")), None);
        assert_eq!(replace_program("git log", "hg"), "hg log");
    }

    #[test]
    fn sed_closes_the_substitution() {
        let output = "sed: -e expression #1, char 9: unterminated `s' command";
        for (script, fixed) in [
            ("sed -e s/foo/bar", "sed -e s/foo/bar/"),
            ("sed -es/foo/bar", "sed -es/foo/bar/"),
            (r"sed -e 's/\/foo/bar'", r"sed -e 's/\/foo/bar/'"),
            (
                "sed -e s/foo/bar -es/baz/quz",
                "sed -e s/foo/bar/ -es/baz/quz/",
            ),
        ] {
            assert_eq!(fixes("sed_unterminated_s", script, output), [fixed]);
        }
        assert!(!rule_matches("sed_unterminated_s", "sed -e s/foo/bar", ""));
        assert!(!rule_matches(
            "sed_unterminated_s",
            "awk -e s/foo/bar",
            output
        ));
    }

    #[test]
    fn ssh_known_hosts_removes_the_key_in_the_shown_command() {
        let old = "@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\n@    WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!     @\n@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\nIT IS POSSIBLE THAT SOMEONE IS DOING SOMETHING NASTY!\nSomeone could be eavesdropping on you right now (man-in-the-middle attack)!\nIt is also possible that a host key has just been changed.\nThe fingerprint for the RSA key sent by the remote host is\nb6:cb:07:34:c0:a0:94:d3:0d:69:83:31:f4:c5:20:9b.\nPlease contact your system administrator.\nAdd correct host key in /tmp/known_hosts to get rid of this message.\nOffending RSA key in /tmp/known_hosts:2\nRSA host key for 98.765.432.321 has changed and you have requested strict checking.\nHost key verification failed.";
        for script in ["ssh user@host", "ssh", "scp something something"] {
            assert!(rule_matches("ssh_known_hosts", script, old), "{script}");
        }
        assert!(!rule_matches("ssh_known_hosts", old, ""));
        assert!(!rule_matches("ssh_known_hosts", "notssh", old));
        assert!(!rule_matches("ssh_known_hosts", "ssh", ""));
        assert_eq!(
            fixes("ssh_known_hosts", "ssh user@host", old),
            ["ssh-keygen -f /tmp/known_hosts -R 98.765.432.321 && ssh user@host"]
        );

        // newer ssh prints the exact command
        let new = "WARNING: REMOTE HOST IDENTIFICATION HAS CHANGED!\nOffending ED25519 key in /home/me/.ssh/known_hosts:7\n  remove with:\n  ssh-keygen -f \"/home/me/.ssh/known_hosts\" -R \"example.com\"\nHost key verification failed.";
        assert_eq!(
            fixes("ssh_known_hosts", "ssh -p 2222 me@example.com", new),
            [
                "ssh-keygen -f /home/me/.ssh/known_hosts -R example.com && ssh -p 2222 me@example.com"
            ]
        );

        // the host from the command when the output does not name one
        let bare = "WARNING: POSSIBLE DNS SPOOFING DETECTED!\nHost key verification failed.";
        assert_eq!(
            fixes(
                "ssh_known_hosts",
                "ssh -p 2222 -i key me@example.com ls",
                bare
            ),
            ["ssh-keygen -R example.com && ssh -p 2222 -i key me@example.com ls"]
        );
        assert_eq!(
            fixes("ssh_known_hosts", "scp file me@example.com:/tmp", bare),
            ["ssh-keygen -R example.com && scp file me@example.com:/tmp"]
        );
    }

    #[test]
    fn switch_lang_retypes_the_command() {
        let aliases = ["welp".to_string(), "fuck".to_string()];
        for (script, fixed) in [
            ("фзе-пуе штыефдд мшь", "apt-get install vim"),
            ("λσ -λα", "ls -la"),
            ("שפא-עקא ןמדאשךך הןצ", "apt-get install vim"),
            ("ךד -ךש", "ls -la"),
            ("멧-ㅎㄷㅅ ㅑㅜㄴㅅ미ㅣ 퍄ㅡ", "apt-get install vim"),
            ("ㅣㄴ -ㅣㅁ", "ls -la"),
            ("ㅔㅁㅅ촤", "patchk"),
        ] {
            assert_eq!(
                switched_layout(script, &aliases).as_deref(),
                Some(fixed),
                "{script}"
            );
        }
        // plain commands, and a word that would turn into the alias
        for script in ["pat-get", "ls", "агсл"] {
            assert_eq!(switched_layout(script, &aliases), None, "{script}");
        }
        for script in ["фзе-пуе", "λσ", "שפא-עקא", "ךד", "녀애 ㅣㄴ"] {
            assert!(rule_matches(
                "switch_lang",
                script,
                &format!("command not found: {script}")
            ));
            assert!(!rule_matches("switch_lang", script, "some info"));
        }
        assert!(!rule_matches("switch_lang", "ls", "command not found: ls"));
    }

    #[test]
    fn systemctl_puts_the_action_first() {
        let output = "Unknown operation 'nginx'.";
        assert_eq!(
            fixes("systemctl", "systemctl nginx start", output),
            ["systemctl start nginx"]
        );
        assert_eq!(
            fixes("systemctl", "sudo systemctl nginx start", output),
            ["sudo systemctl start nginx"]
        );
        for (script, output) in [
            ("systemctl start nginx", ""),
            ("sudo systemctl nginx", output),
            ("systemctl nginx", output),
            (
                "systemctl start wtf",
                "Failed to start wtf.service: Unit wtf.service failed to load: No such file or directory.",
            ),
        ] {
            assert!(!rule_matches("systemctl", script, output), "{script}");
        }
    }

    #[test]
    fn tmux_picks_the_closest_commands() {
        let output = "ambiguous command: list, could be: list-buffers, list-clients, list-commands, list-keys, list-panes, list-sessions, list-windows";
        assert_eq!(
            fixes("tmux", "tmux list", output),
            ["tmux list-keys", "tmux list-panes", "tmux list-windows"]
        );
        assert!(!rule_matches("tmux", "tmux list-keys", ""));
        assert!(!rule_matches("tmux", "screen list", output));
    }

    #[test]
    fn unknown_command_adds_the_dash() {
        let output = |name: &str| {
            format!(
                "{name}: Unknown command\nDid you mean -{name}?  This command begins with a dash."
            )
        };
        for (script, fixed) in [
            ("hdfs dfs ls", "hdfs dfs -ls"),
            ("hdfs dfs ls /foo/bar", "hdfs dfs -ls /foo/bar"),
            (
                "./bin/hdfs dfs ls -R /foo/bar",
                "./bin/hdfs dfs -ls -R /foo/bar",
            ),
            (
                "./bin/hdfs dfs -Dtest=fred ls -R /foo/bar",
                "./bin/hdfs dfs -Dtest=fred -ls -R /foo/bar",
            ),
        ] {
            assert_eq!(fixes("unknown_command", script, &output("ls")), [fixed]);
        }
        assert_eq!(
            fixes("unknown_command", "hdfs dfs rm /foo/bar", &output("rm")),
            ["hdfs dfs -rm /foo/bar"]
        );
        for script in [
            "./bin/hdfs dfs -ls",
            "./bin/hdfs dfs -ls /foo/bar",
            "hdfs dfs -ls -R /foo/bar",
            "",
        ] {
            assert!(!rule_matches("unknown_command", script, ""), "{script}");
        }
    }

    #[test]
    fn whois_asks_about_the_domain() {
        assert_eq!(
            fixes("whois", "whois https://en.wikipedia.org/wiki/Main_Page", ""),
            ["whois en.wikipedia.org"]
        );
        assert_eq!(
            fixes("whois", "whois https://en.wikipedia.org/", ""),
            ["whois en.wikipedia.org"]
        );
        assert_eq!(
            fixes("whois", "whois meta.unix.stackexchange.com", ""),
            [
                "whois unix.stackexchange.com",
                "whois stackexchange.com",
                "whois com"
            ]
        );
        assert_eq!(
            fixes("whois", "whois en.wikipedia.org/wiki/Main_Page", ""),
            ["whois en.wikipedia.org"]
        );
        assert!(!rule_matches("whois", "whois", ""));
        assert!(!rule_matches("whois", "echo whois.example.com", ""));
    }

    #[test]
    fn wrong_hyphen_becomes_a_space() {
        let executable = |name: &str| ["git", "apt", "apt-get", "ls", "pwd"].contains(&name);
        for script in ["git-log", "apt-install python", "sudo git-log"] {
            assert!(
                wrong_hyphen(&Command::new(script, ""), executable),
                "{script}"
            );
        }
        for script in ["ls -la", "git2-make", "apt-get install python"] {
            assert!(
                !wrong_hyphen(&Command::new(script, ""), executable),
                "{script}"
            );
        }
        // without a hyphen in the program there is nothing to fix, whatever is installed
        assert!(!rule_matches(
            "wrong_hyphen_before_subcommand",
            "ls -la",
            ""
        ));
    }
}

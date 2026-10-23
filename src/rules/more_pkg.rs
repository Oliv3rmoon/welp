//! More package manager and platform rules ported from thefuck: apt, brew,
//! choco, dnf, nix, the *env version managers, pacman and yum.
//!
//! Where thefuck runs a tool to learn the valid names, the rule here runs the
//! same help or listing command through `listing` (read-only, 3 s timeout),
//! or reads the directory the information lives in.

use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

use super::more_tools::{is_app, on_path, replace_command, unsudo, with_sudo};
use super::{Rule, listing, replace_arg};
use crate::command::Command;

/// Rules in this file that are off unless the user enables them.
pub const DISABLED: &[&str] = &[];

pub fn rules() -> Vec<Rule> {
    vec![
        Rule {
            name: "apt_get_search",
            description: "Searches with apt-cache, since apt-get has no search",
            priority: 1000,
            matches: |c| is_app(c, &["apt-get"]) && c.script.starts_with("apt-get search"),
            fixes: |c| vec![format!("apt-cache{}", &c.script["apt-get".len()..])],
        },
        Rule {
            name: "apt_invalid_operation",
            description: "Fixes a mistyped apt operation",
            priority: 1100,
            matches: |c| {
                is_app(c, &["apt", "apt-get", "apt-cache"])
                    && c.output.contains("E: Invalid operation")
            },
            fixes: |c| with_sudo(c, apt_invalid_operation),
        },
        Rule {
            name: "apt_list_upgradable",
            description: "Lists the upgradable packages apt mentions",
            priority: 1000,
            matches: |c| is_app(c, &["apt"]) && c.output.contains("apt list --upgradable"),
            fixes: |c| with_sudo(c, |_| vec!["apt list --upgradable".to_string()]),
        },
        Rule {
            name: "apt_upgrade",
            description: "Upgrades after listing the upgradable packages",
            priority: 1000,
            matches: |c| is_app(c, &["apt"]) && apt_upgrade_applies(&unsudo(c).1),
            fixes: |c| with_sudo(c, |_| vec!["apt upgrade".to_string()]),
        },
        Rule {
            name: "brew_cask_dependency",
            description: "Installs the cask a formula needs first",
            priority: 1000,
            matches: |c| {
                is_app(c, &["brew"])
                    && c.parts.iter().any(|part| part == "install")
                    && c.output.contains("brew cask install")
            },
            fixes: |c| {
                let casks: Vec<&str> = c
                    .output
                    .split('\n')
                    .map(str::trim)
                    .filter(|line| line.starts_with("brew cask install"))
                    .collect();
                if casks.is_empty() {
                    Vec::new()
                } else {
                    vec![format!("{} && {}", casks.join(" && "), c.script)]
                }
            },
        },
        Rule {
            name: "brew_install",
            description: "Installs the formula brew suggests",
            priority: 1000,
            matches: |c| {
                is_app(c, &["brew"])
                    && c.parts.len() > 2
                    && c.script.contains("install")
                    && c.output.contains("No available formula")
                    && c.output.contains("Did you mean")
            },
            fixes: |c| {
                BREW_FORMULA
                    .captures(&c.output)
                    .map(|found| brew_suggestions(&found[1]))
                    .unwrap_or_default()
                    .iter()
                    .map(|formula| format!("brew install {}", shell_words::quote(formula)))
                    .collect()
            },
        },
        Rule {
            name: "brew_unknown_command",
            description: "Fixes a mistyped brew command",
            priority: 1100,
            matches: |c| {
                c.script.contains("brew")
                    && c.output.contains("Unknown command")
                    && BREW_UNKNOWN.is_match(&c.output)
            },
            fixes: |c| brew_unknown_command(c, &brew_commands()),
        },
        Rule {
            name: "brew_update_formula",
            description: "Upgrades the formula, since brew update only updates brew",
            priority: 1000,
            matches: |c| {
                is_app(c, &["brew"])
                    && c.parts.len() > 2
                    && c.script.contains("update")
                    && c.output.contains("Error: This command updates brew itself")
                    && c.output.contains("Use `brew upgrade")
            },
            fixes: |c| vec![replace_arg(&c.script, "update", "upgrade")],
        },
        Rule {
            name: "choco_install",
            description: "Installs the .install package of the same name",
            priority: 1000,
            matches: |c| {
                is_app(c, &["choco", "cinst"])
                    && (c.script.starts_with("choco install")
                        || c.parts.iter().any(|part| part == "cinst"))
                    && c.output.contains("Installing the following packages")
            },
            fixes: |c| {
                c.parts
                    .iter()
                    .find(|part| is_choco_package(part))
                    .map(|package| replace_arg(&c.script, package, &format!("{package}.install")))
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "dnf_no_such_command",
            description: "Fixes a mistyped dnf command",
            priority: 1100,
            matches: |c| is_app(c, &["dnf"]) && c.output_lower().contains("no such command"),
            fixes: |c| {
                with_sudo(c, |c| {
                    dnf_no_such_command(c, listing("dnf", &["--help"]).as_deref())
                })
            },
        },
        Rule {
            name: "nixos_cmd_not_found",
            description: "Installs the program with nix-env, as NixOS suggests, then runs it",
            priority: 1000,
            matches: |c| NIX_INSTALL.is_match(&c.output),
            fixes: |c| {
                NIX_INSTALL
                    .captures(&c.output)
                    .map(|found| {
                        format!(
                            "nix-env -iA {} && {}",
                            shell_words::quote(&found[1]),
                            c.script
                        )
                    })
                    .into_iter()
                    .collect()
            },
        },
        Rule {
            name: "omnienv_no_such_command",
            description: "Fixes a mistyped goenv, nodenv, pyenv or rbenv command",
            priority: 1100,
            matches: |c| {
                is_app(c, &OMNIENV_APPS)
                    && c.parts.len() > 1
                    && c.output.contains("env: no such command ")
            },
            fixes: |c| {
                let app = c
                    .parts
                    .first()
                    .map_or("", |first| first.rsplit('/').next().unwrap_or(first));
                omnienv_no_such_command(c, listing(app, &["commands"]).as_deref())
            },
        },
        Rule {
            name: "pacman",
            description: "Installs the package that has the missing program, then runs it",
            priority: 1000,
            matches: |c| {
                pacman_command().is_some()
                    && c.output.contains("not found")
                    && !pkgfile_packages(&c.script).is_empty()
            },
            fixes: |c| {
                pacman_command()
                    .map(|pacman| pacman_install_fixes(c, pacman, &pkgfile_packages(&c.script)))
                    .unwrap_or_default()
            },
        },
        Rule {
            name: "pacman_invalid_option",
            description: "Capitalizes pacman's operation flag: -s to -S",
            priority: 1000,
            matches: |c| is_app(c, &["pacman"]) && pacman_invalid_option_applies(&unsudo(c).1),
            fixes: |c| with_sudo(c, pacman_invalid_option),
        },
        Rule {
            name: "pacman_not_found",
            description: "Fixes a package name pacman does not know",
            priority: 1100,
            matches: |c| {
                (is_app(c, &["pacman", "yay", "pikaur", "yaourt"]))
                    && c.output.contains("error: target not found:")
            },
            fixes: |c| {
                c.parts
                    .last()
                    .map(|name| pacman_not_found(c, &pkgfile_packages(name)))
                    .unwrap_or_default()
            },
        },
        Rule {
            name: "yum_invalid_operation",
            description: "Fixes a mistyped yum operation",
            priority: 1100,
            matches: |c| is_app(c, &["yum"]) && c.output.contains("No such command: "),
            fixes: |c| {
                with_sudo(c, |c| {
                    yum_invalid_operation(c, listing("yum", &[]).as_deref())
                })
            },
        },
    ]
}

// apt

fn apt_upgrade_applies(command: &Command) -> bool {
    command.script == "apt list --upgradable" && command.output.trim().split('\n').count() > 1
}

/// The operations in `apt --help`: lines like ` list - list packages`.
fn apt_operations(help: &str) -> Vec<String> {
    let mut listed = false;
    let mut operations = Vec::new();
    for line in help.lines().map(str::trim) {
        if listed {
            let mut words = line.split_whitespace();
            if let (Some(name), Some("-")) = (words.next(), words.next()) {
                operations.push(name.to_string());
            }
        } else if line.starts_with("Basic commands:") || line.starts_with("Most used commands:") {
            listed = true;
        }
    }
    operations
}

/// The operations in `apt-get --help` and `apt-cache --help`: the lines
/// under "Commands:" up to the next empty one.
fn apt_get_operations(help: &str) -> Vec<String> {
    help.lines()
        .map(str::trim)
        .skip_while(|line| {
            !(line.starts_with("Commands:") || line.starts_with("Most used commands:"))
        })
        .skip(1)
        .take_while(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect()
}

fn apt_invalid_operation(command: &Command) -> Vec<String> {
    let Some(invalid) = command.output.split_whitespace().last() else {
        return Vec::new();
    };
    if invalid == "uninstall" {
        return vec![replace_arg(&command.script, "uninstall", "remove")];
    }
    let program = command.program().rsplit('/').next().unwrap_or_default();
    let Some(help) = listing(program, &["--help"]) else {
        return Vec::new();
    };
    let operations = if program == "apt" {
        apt_operations(&help)
    } else {
        apt_get_operations(&help)
    };
    replace_command(&command.script, invalid, &operations)
}

// brew

static BREW_FORMULA: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r#"(?:Warning|Error): No available formula with the name "(?:[^"]+)". Did you mean (.+)\?"#,
    )
    .unwrap()
});
static BREW_UNKNOWN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"Error: (?:Invalid usage: )?Unknown command: (?:brew )?([a-z]+)").unwrap()
});

/// "git, gitg or gist" as a list of names.
fn brew_suggestions(text: &str) -> Vec<String> {
    text.replace(" or ", ", ")
        .split(", ")
        .map(String::from)
        .collect()
}

/// brew's fallback list when it can't be asked (Homebrew 0.9.5).
const BREW_COMMANDS: &[&str] = &[
    "info",
    "home",
    "options",
    "install",
    "uninstall",
    "search",
    "list",
    "update",
    "upgrade",
    "pin",
    "unpin",
    "doctor",
    "create",
    "edit",
    "cask",
];

/// The names of the `.rb` and `.sh` files in a directory, without the extension.
fn script_names(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter_map(|name| {
            name.strip_suffix(".rb")
                .or_else(|| name.strip_suffix(".sh"))
                .map(String::from)
        })
        .collect()
}

fn subdirectories(dir: &Path) -> Vec<String> {
    fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .collect()
}

/// The commands brew has under its prefix: its own, and the ones taps add.
/// The Homebrew checkout is in `<prefix>/Homebrew` on Intel Macs and in the
/// prefix itself on Apple silicon.
fn brew_commands_under(prefix: &Path) -> Option<Vec<String>> {
    let library = [prefix.join("Homebrew/Library"), prefix.join("Library")]
        .into_iter()
        .find(|library| library.join("Homebrew/cmd").is_dir())?;
    let mut commands = script_names(&library.join("Homebrew/cmd"));
    let taps = library.join("Taps");
    for user in subdirectories(&taps) {
        for tap in subdirectories(&taps.join(&user)) {
            if !tap.starts_with("homebrew-") {
                continue;
            }
            let dir = taps.join(&user).join(&tap).join("cmd");
            commands.extend(
                fs::read_dir(&dir)
                    .into_iter()
                    .flatten()
                    .flatten()
                    .filter_map(|entry| {
                        let name = entry.file_name().into_string().ok()?;
                        let command = name.strip_prefix("brew-")?.strip_suffix(".rb")?;
                        Some(command.to_string())
                    }),
            );
        }
    }
    commands.sort();
    commands.dedup();
    Some(commands)
}

fn brew_commands() -> Vec<String> {
    listing("brew", &["--prefix"])
        .and_then(|prefix| brew_commands_under(Path::new(prefix.trim())))
        .unwrap_or_else(|| BREW_COMMANDS.iter().map(|name| name.to_string()).collect())
}

fn brew_unknown_command(command: &Command, commands: &[String]) -> Vec<String> {
    BREW_UNKNOWN
        .captures(&command.output)
        .map(|broken| replace_command(&command.script, &broken[1], commands))
        .unwrap_or_default()
}

// choco

/// Whether a word of `choco install` is the package: not the program, the
/// subcommand, a flag or a `key=value`/path parameter.
fn is_choco_package(word: &str) -> bool {
    !["choco", "cinst", "install"].contains(&word)
        && !word.starts_with('-')
        && !word.contains('=')
        && !word.contains('/')
}

// dnf and yum

static DNF_UNKNOWN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"No such command: (\S+)\.").unwrap());
static DNF_OPERATION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?m)^([a-z-]+) +").unwrap());

fn dnf_operations(help: &str) -> Vec<String> {
    DNF_OPERATION
        .captures_iter(help)
        .map(|found| found[1].to_string())
        .collect()
}

fn dnf_no_such_command(command: &Command, help: Option<&str>) -> Vec<String> {
    let (Some(wrong), Some(help)) = (DNF_UNKNOWN.captures(&command.output), help) else {
        return Vec::new();
    };
    replace_command(&command.script, &wrong[1], &dnf_operations(help))
}

/// The operations under "List of Commands:" in yum's help.
fn yum_operations(help: &str) -> Vec<String> {
    help.lines()
        .skip_while(|line| !line.starts_with("List of Commands:"))
        .skip(2)
        .take_while(|line| !line.trim().is_empty())
        .filter_map(|line| line.trim().split(' ').next().map(String::from))
        .collect()
}

fn yum_invalid_operation(command: &Command, help: Option<&str>) -> Vec<String> {
    let Some(invalid) = command.parts.get(1) else {
        return Vec::new();
    };
    if invalid == "uninstall" {
        return vec![replace_arg(&command.script, "uninstall", "remove")];
    }
    help.map(|help| replace_command(&command.script, invalid, &yum_operations(help)))
        .unwrap_or_default()
}

// nix and the *env tools

static NIX_INSTALL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"nix-env -iA ([^\s]*)").unwrap());

const OMNIENV_APPS: [&str; 4] = ["goenv", "nodenv", "pyenv", "rbenv"];

static OMNIENV_BROKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"env: no such command ['`]([^']*)'").unwrap());

/// Mistakes that come from other tools' vocabulary, and what is meant.
fn omnienv_typos(broken: &str) -> &'static [&'static str] {
    match broken {
        "list" => &["versions", "install --list"],
        "remove" => &["uninstall"],
        _ => &[],
    }
}

fn omnienv_no_such_command(command: &Command, commands: Option<&str>) -> Vec<String> {
    let Some(broken) = OMNIENV_BROKEN.captures(&command.output) else {
        return Vec::new();
    };
    let broken = &broken[1];
    let mut fixes: Vec<String> = omnienv_typos(broken)
        .iter()
        .map(|typo| replace_arg(&command.script, broken, typo))
        .collect();
    let names: Vec<&str> = commands
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect();
    fixes.extend(replace_command(&command.script, broken, &names));
    fixes
}

// pacman

/// What thefuck calls `pacman` in its Arch rules: an AUR helper if there is
/// one, else pacman itself with sudo. `None` when this isn't an Arch box.
fn pacman_command() -> Option<&'static str> {
    ["yay", "pikaur", "yaourt"]
        .into_iter()
        .find(|helper| on_path(helper))
        .or_else(|| on_path("pacman").then_some("sudo pacman"))
}

/// The packages (`extra/vim`) that hold a program, from `pkgfile`. The
/// program is the first word of `script`, past any `sudo`.
fn pkgfile_packages(script: &str) -> Vec<String> {
    let script = script.trim();
    let script = script.strip_prefix("sudo ").unwrap_or(script);
    let Some(program) = script
        .split(' ')
        .next()
        .filter(|word| !word.starts_with('-'))
    else {
        return Vec::new();
    };
    listing("pkgfile", &["-b", "-v", program])
        .map(|found| parse_pkgfile(&found))
        .unwrap_or_default()
}

fn parse_pkgfile(found: &str) -> Vec<String> {
    found
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(String::from))
        .collect()
}

fn pacman_install_fixes(command: &Command, pacman: &str, packages: &[String]) -> Vec<String> {
    packages
        .iter()
        .map(|package| format!("{pacman} -S {package} && {}", command.script))
        .collect()
}

static PACMAN_OPTION: LazyLock<Regex> = LazyLock::new(|| Regex::new(r" -[dfqrstuv]").unwrap());

fn pacman_invalid_option_applies(command: &Command) -> bool {
    command.output.starts_with("error: invalid option '-")
        && "surqfdvt"
            .chars()
            .any(|option| command.script.contains(&format!(" -{option}")))
}

fn pacman_invalid_option(command: &Command) -> Vec<String> {
    PACMAN_OPTION
        .find(&command.script)
        .map(|option| {
            command
                .script
                .replace(option.as_str(), &option.as_str().to_uppercase())
        })
        .into_iter()
        .collect()
}

fn pacman_not_found(command: &Command, packages: &[String]) -> Vec<String> {
    command
        .parts
        .last()
        .map(|name| replace_command(&command.script, name, packages))
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::{all, rule_matches};

    const APT_HELP: &str = r#"apt 1.0.10.2ubuntu1 for amd64 compiled on Oct  5 2015 15:55:05
Usage: apt [options] command

CLI for apt.
Basic commands:
 list - list packages based on package names
 search - search in package descriptions
 show - show package details

 update - update list of available packages

 install - install packages
 remove  - remove packages

 upgrade - upgrade the system by installing/upgrading packages
 full-upgrade - upgrade the system by removing/installing/upgrading packages

 edit-sources - edit the source information file
"#;
    const APT_GET_HELP: &str = r#"apt 1.0.10.2ubuntu1 for amd64 compiled on Oct  5 2015 15:55:05
Usage: apt-get [options] command
       apt-get [options] install|remove pkg1 [pkg2 ...]
       apt-get [options] source pkg1 [pkg2 ...]

apt-get is a simple command line interface for downloading and
installing packages. The most frequently used commands are update
and install.

Commands:
   update - Retrieve new lists of packages
   upgrade - Perform an upgrade
   install - Install new packages (pkg is libc6 not libc6.deb)
   remove - Remove packages
   autoremove - Remove automatically all unused packages
   purge - Remove packages and config files
   source - Download source archives
   build-dep - Configure build-dependencies for source packages
   dist-upgrade - Distribution upgrade, see apt-get(8)
   dselect-upgrade - Follow dselect selections
   clean - Erase downloaded archive files
   autoclean - Erase old downloaded archive files
   check - Verify that there are no broken dependencies
   changelog - Download and display the changelog for the given package
   download - Download the binary package into the current directory

Options:
  -h  This help text.
  -q  Loggable output - no progress indicator
  -qq No output except for errors
  -d  Download only - do NOT install or unpack archives
  -s  No-act. Perform ordering simulation
  -y  Assume Yes to all queries and do not prompt
  -f  Attempt to correct a system with broken dependencies in place
  -m  Attempt to continue if archives are unlocatable
  -u  Show a list of upgraded packages as well
  -b  Build the source package after fetching it
  -V  Show verbose version numbers
  -c=? Read this configuration file
  -o=? Set an arbitrary configuration option, eg -o dir::cache=/tmp
See the apt-get(8), sources.list(5) and apt.conf(5) manual
pages for more information and options.
                       This APT has Super Cow Powers.
"#;
    const NEW_APT_GET_HELP: &str = r#"apt 1.6.12 (amd64)
Usage: apt-get [options] command
       apt-get [options] install|remove pkg1 [pkg2 ...]
       apt-get [options] source pkg1 [pkg2 ...]

apt-get is a command line interface for retrieval of packages
and information about them from authenticated sources and
for installation, upgrade and removal of packages together
with their dependencies.

Most used commands:
  update - Retrieve new lists of packages
  upgrade - Perform an upgrade
  install - Install new packages (pkg is libc6 not libc6.deb)
  remove - Remove packages
  purge - Remove packages and config files
  autoremove - Remove automatically all unused packages
  dist-upgrade - Distribution upgrade, see apt-get(8)
  dselect-upgrade - Follow dselect selections
  build-dep - Configure build-dependencies for source packages
  clean - Erase downloaded archive files
  autoclean - Erase old downloaded archive files
  check - Verify that there are no broken dependencies
  source - Download source archives
  download - Download the binary package into the current directory
  changelog - Download and display the changelog for the given package

See apt-get(8) for more information about the available commands.
Configuration options and syntax is detailed in apt.conf(5).
Information about how to configure sources can be found in sources.list(5).
Package and version choices can be expressed via apt_preferences(5).
Security details are available in apt-secure(8).
                                        This APT has Super Cow Powers.
"#;
    const DNF_HELP: &str = r#"usage: dnf [options] COMMAND

List of Main Commands:

autoremove                remove all unneeded packages that were originally installed as dependencies
check                     check for problems in the packagedb
check-update              check for available package upgrades
clean                     remove cached data
deplist                   List package's dependencies and what packages provide them
distro-sync               synchronize installed packages to the latest available versions
downgrade                 Downgrade a package
group                     display, or use, the groups information
help                      display a helpful usage message
history                   display, or use, the transaction history
info                      display details about a package or group of packages
install                   install a package or packages on your system
list                      list a package or groups of packages
makecache                 generate the metadata cache
mark                      mark or unmark installed packages as installed by user.
provides                  find what package provides the given value
reinstall                 reinstall a package
remove                    remove a package or packages from your system
repolist                  display the configured software repositories
repoquery                 search for packages matching keyword
repository-packages       run commands on top of all packages in given repository
search                    search package details for the given string
shell                     run an interactive DNF shell
swap                      run an interactive dnf mod for remove and install one spec
updateinfo                display advisories about packages
upgrade                   upgrade a package or packages on your system
upgrade-minimal           upgrade, but only 'newest' package match which fixes a problem that affects your system

List of Plugin Commands:

builddep                  Install build dependencies for package or spec file
config-manager            manage dnf configuration options and repositories
copr                      Interact with Copr repositories.
debug-dump                dump information about installed rpm packages to file
debug-restore             restore packages recorded in debug-dump file
debuginfo-install         install debuginfo packages
download                  Download package to current directory
needs-restarting          determine updated binaries that need restarting
playground                Interact with Playground repository.
repoclosure               Display a list of unresolved dependencies for repositories
repograph                 Output a full package dependency graph in dot format
repomanage                Manage a directory of rpm packages
reposync                  download all packages from remote repo

Optional arguments:
  -c [config file], --config [config file]
                        config file location
  -q, --quiet           quiet operation
  -v, --verbose         verbose operation
  --version             show DNF version and exit
  --installroot [path]  set install root
  --nodocs              do not install documentations
  --noplugins           disable all plugins
  --enableplugin [plugin]
                        enable plugins by name
  --disableplugin [plugin]
                        disable plugins by name
  --releasever RELEASEVER
                        override the value of $releasever in config and repo
                        files
  --setopt SETOPTS      set arbitrary config and repo options
  --skip-broken         resolve depsolve problems by skipping packages
  -h, --help, --help-cmd
                        show command help
  --allowerasing        allow erasing of installed packages to resolve
                        dependencies
  -b, --best            try the best available package versions in
                        transactions.
  -C, --cacheonly       run entirely from system cache, don't update cache
  -R [minutes], --randomwait [minutes]
                        maximum command wait time
  -d [debug level], --debuglevel [debug level]
                        debugging output level
  --debugsolver         dumps detailed solving results into files
  --showduplicates      show duplicates, in repos, in list/search commands
  -e ERRORLEVEL, --errorlevel ERRORLEVEL
                        error output level
  --obsoletes           enables dnf's obsoletes processing logic for upgrade
                        or display capabilities that the package obsoletes for
                        info, list and repoquery
  --rpmverbosity [debug level name]
                        debugging output level for rpm
  -y, --assumeyes       automatically answer yes for all questions
  --assumeno            automatically answer no for all questions
  --enablerepo [repo]
  --disablerepo [repo]
  --repo [repo], --repoid [repo]
                        enable just specific repositories by an id or a glob,
                        can be specified multiple times
  -x [package], --exclude [package], --excludepkgs [package]
                        exclude packages by name or glob
  --disableexcludes [repo], --disableexcludepkgs [repo]
                        disable excludepkgs
  --repofrompath [repo,path]
                        label and path to additional repository, can be
                        specified multiple times.
  --noautoremove        disable removal of dependencies that are no longer
                        used
  --nogpgcheck          disable gpg signature checking
  --color COLOR         control whether colour is used
  --refresh             set metadata as expired before running the command
  -4                    resolve to IPv4 addresses only
  -6                    resolve to IPv6 addresses only
  --destdir DESTDIR, --downloaddir DESTDIR
                        set directory to copy packages to
  --downloadonly        only download packages
  --bugfix              Include bugfix relevant packages, in updates
  --enhancement         Include enhancement relevant packages, in updates
  --newpackage          Include newpackage relevant packages, in updates
  --security            Include security relevant packages, in updates
  --advisory ADVISORY, --advisories ADVISORY
                        Include packages needed to fix the given advisory, in
                        updates
  --bzs BUGZILLA        Include packages needed to fix the given BZ, in
                        updates
  --cves CVES           Include packages needed to fix the given CVE, in
                        updates
  --sec-severity {Critical,Important,Moderate,Low}, --secseverity {Critical,Important,Moderate,Low}
                        Include security relevant packages matching the
                        severity, in updates
  --forcearch ARCH      Force the use of an architecture
"#;
    const YUM_HELP: &str = r#"Loaded plugins: extras_suggestions, langpacks, priorities, update-motd
Usage: yum [options] COMMAND

List of Commands:

check          Check for problems in the rpmdb
check-update   Check for available package updates
clean          Remove cached data
deplist        List a package's dependencies
distribution-synchronization Synchronize installed packages to the latest available versions
downgrade      downgrade a package
erase          Remove a package or packages from your system
fs             Acts on the filesystem data of the host, mainly for removing docs/lanuages for minimal hosts.
fssnapshot     Creates filesystem snapshots, or lists/deletes current snapshots.
groups         Display, or use, the groups information
help           Display a helpful usage message
history        Display, or use, the transaction history
info           Display details about a package or group of packages
install        Install a package or packages on your system
langavailable  Check available languages
langinfo       List languages information
langinstall    Install appropriate language packs for a language
langlist       List installed languages
langremove     Remove installed language packs for a language
list           List a package or groups of packages
load-transaction load a saved transaction from filename
makecache      Generate the metadata cache
provides       Find what package provides the given value
reinstall      reinstall a package
repo-pkgs      Treat a repo. as a group of packages, so we can install/remove all of them
repolist       Display the configured software repositories
search         Search package details for the given string
shell          Run an interactive yum shell
swap           Simple way to swap packages, instead of using shell
update         Update a package or packages on your system
update-minimal Works like upgrade, but goes to the 'newest' package match which fixes a problem that affects your system
updateinfo     Acts on repository update information
upgrade        Update packages taking obsoletes into account
version        Display a version for the machine and/or available repos.


Options:
  -h, --help            show this help message and exit
  -t, --tolerant        be tolerant of errors
  -C, --cacheonly       run entirely from system cache, don't update cache
  -c [config file], --config=[config file]
                        config file location
  -R [minutes], --randomwait=[minutes]
                        maximum command wait time
  -d [debug level], --debuglevel=[debug level]
                        debugging output level
  --showduplicates      show duplicates, in repos, in list/search commands
  -e [error level], --errorlevel=[error level]
                        error output level
  --rpmverbosity=[debug level name]
                        debugging output level for rpm
  -q, --quiet           quiet operation
  -v, --verbose         verbose operation
  -y, --assumeyes       answer yes for all questions
  --assumeno            answer no for all questions
  --version             show Yum version and exit
  --installroot=[path]  set install root
  --enablerepo=[repo]   enable one or more repositories (wildcards allowed)
  --disablerepo=[repo]  disable one or more repositories (wildcards allowed)
  -x [package], --exclude=[package]
                        exclude package(s) by name or glob
  --disableexcludes=[repo]
                        disable exclude from main, for a repo or for
                        everything
  --disableincludes=[repo]
                        disable includepkgs for a repo or for everything
  --obsoletes           enable obsoletes processing during updates
  --noplugins           disable Yum plugins
  --nogpgcheck          disable gpg signature checking
  --disableplugin=[plugin]
                        disable plugins by name
  --enableplugin=[plugin]
                        enable plugins by name
  --skip-broken         skip packages with depsolving problems
  --color=COLOR         control whether color is used
  --releasever=RELEASEVER
                        set value of $releasever in yum config and repo files
  --downloadonly        don't update, just download
  --downloaddir=DLDIR   specifies an alternate directory to store packages
  --setopt=SETOPTS      set arbitrary config and repo options
  --bugfix              Include bugfix relevant packages, in updates
  --security            Include security relevant packages, in updates
  --advisory=ADVS, --advisories=ADVS
                        Include packages needed to fix the given advisory, in
                        updates
  --bzs=BZS             Include packages needed to fix the given BZ, in
                        updates
  --cves=CVES           Include packages needed to fix the given CVE, in
                        updates
  --sec-severity=SEVS, --secseverity=SEVS
                        Include security relevant packages matching the
                        severity, in updates

  Plugin Options:
    --samearch-priorities
                        Priority-exclude packages based on name + arch
"#;

    /// What the named rule alone offers: nothing unless it matches. Only for
    /// rules whose fixes don't run a program; the others are tested through
    /// the function that takes the program's output.
    fn fixes_of(name: &str, script: &str, output: &str) -> Vec<String> {
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

    #[test]
    fn apt_get_has_no_search() {
        assert_eq!(
            fixes_of("apt_get_search", "apt-get search foo", ""),
            ["apt-cache search foo"]
        );
        for script in [
            "apt-cache search foo",
            "aptitude search foo",
            "apt search foo",
            "apt-get install foo",
            "apt-get source foo",
            "apt-get clean",
            "apt-get remove",
            "apt-get update",
        ] {
            assert!(!rule_matches("apt_get_search", script, ""), "{script}");
        }
    }

    #[test]
    fn apt_fixes_a_mistyped_operation() {
        let invalid = |operation: &str| format!("E: Invalid operation {operation}");
        for (script, operation) in [
            ("apt", "saerch"),
            ("apt-get", "isntall"),
            ("apt-cache", "rumove"),
        ] {
            assert!(rule_matches(
                "apt_invalid_operation",
                script,
                &invalid(operation)
            ));
        }
        assert!(!rule_matches(
            "apt_invalid_operation",
            "vim",
            &invalid("vim")
        ));
        assert!(!rule_matches("apt_invalid_operation", "apt-get", ""));

        // what the three help texts list
        assert_eq!(
            apt_operations(APT_HELP),
            [
                "list",
                "search",
                "show",
                "update",
                "install",
                "remove",
                "upgrade",
                "full-upgrade",
                "edit-sources"
            ]
        );
        let apt_get = [
            "update",
            "upgrade",
            "install",
            "remove",
            "autoremove",
            "purge",
            "source",
            "build-dep",
            "dist-upgrade",
            "dselect-upgrade",
            "clean",
            "autoclean",
            "check",
            "changelog",
            "download",
        ];
        assert_eq!(apt_get_operations(APT_GET_HELP), apt_get);
        assert_eq!(
            apt_get_operations(NEW_APT_GET_HELP),
            [
                "update",
                "upgrade",
                "install",
                "remove",
                "purge",
                "autoremove",
                "dist-upgrade",
                "dselect-upgrade",
                "build-dep",
                "clean",
                "autoclean",
                "check",
                "source",
                "download",
                "changelog"
            ]
        );

        // fixes, with those lists standing in for the help
        let first = |script: &str, operations: Vec<String>| {
            let wrong = script.split(' ').nth(1).unwrap();
            replace_command(script, wrong, &operations)[0].clone()
        };
        assert_eq!(
            first("apt-get isntall vim", apt_get_operations(APT_GET_HELP)),
            "apt-get install vim"
        );
        assert_eq!(
            first("apt saerch vim", apt_operations(APT_HELP)),
            "apt search vim"
        );
        // uninstall needs no list
        let uninstall = Command::new("apt uninstall vim", &invalid("uninstall"));
        assert_eq!(apt_invalid_operation(&uninstall), ["apt remove vim"]);
        assert_eq!(
            with_sudo(
                &Command::new("sudo apt uninstall vim", &invalid("uninstall")),
                apt_invalid_operation
            ),
            ["sudo apt remove vim"]
        );
    }

    #[test]
    fn apt_lists_what_it_says_is_upgradable() {
        let english = "Hit:1 http://us.archive.ubuntu.com/ubuntu zesty InRelease\nFetched 89.2 kB in 0s (122 kB/s)\nReading package lists... Done\nBuilding dependency tree\nReading state information... Done\n8 packages can be upgraded. Run 'apt list --upgradable' to see them.\n";
        let german = "Führen Sie »apt list --upgradable« aus, um sie anzuzeigen.";
        for output in [english, german] {
            assert_eq!(
                fixes_of("apt_list_upgradable", "sudo apt update", output),
                ["sudo apt list --upgradable"]
            );
            assert_eq!(
                fixes_of("apt_list_upgradable", "apt update", output),
                ["apt list --upgradable"]
            );
        }
        let current = "Hit:1 http://us.archive.ubuntu.com/ubuntu zesty InRelease\nReading package lists... Done\nAll packages are up to date.\n";
        assert!(!rule_matches(
            "apt_list_upgradable",
            "sudo apt update",
            current
        ));
        assert!(!rule_matches("apt_list_upgradable", "apt-get update", ""));
        assert!(!rule_matches("apt_list_upgradable", "apt search foo", ""));
    }

    #[test]
    fn apt_upgrades_after_the_list() {
        let upgradable = "\nListing... Done\nheroku/stable 6.15.2-1 amd64 [upgradable from: 6.14.43-1]\nresolvconf/zesty-updates,zesty-updates 1.79ubuntu4.1 all [upgradable from: 1.79ubuntu4]\n";
        assert_eq!(
            fixes_of("apt_upgrade", "apt list --upgradable", upgradable),
            ["apt upgrade"]
        );
        assert_eq!(
            fixes_of("apt_upgrade", "sudo apt list --upgradable", upgradable),
            ["sudo apt upgrade"]
        );
        for script in ["apt list --upgradable", "sudo apt list --upgradable"] {
            assert!(!rule_matches("apt_upgrade", script, "\nListing... Done\n"));
        }
    }

    #[test]
    fn brew_installs_the_cask_first() {
        let output = "sshfs: OsxfuseRequirement unsatisfied!\n\nYou can install with Homebrew-Cask:\n  brew cask install osxfuse\n\nYou can download from:\n  https://osxfuse.github.io/\nError: An unsatisfied requirement failed this build.";
        assert_eq!(
            fixes_of("brew_cask_dependency", "brew install sshfs", output),
            ["brew cask install osxfuse && brew install sshfs"]
        );
        let two = "  brew cask install osxfuse\n  brew cask install java\n";
        assert_eq!(
            fixes_of("brew_cask_dependency", "brew install sshfs", two),
            ["brew cask install osxfuse && brew cask install java && brew install sshfs"]
        );
        assert!(!rule_matches(
            "brew_cask_dependency",
            "brew link sshfs",
            output
        ));
        assert!(!rule_matches("brew_cask_dependency", "cat output", output));
        assert!(!rule_matches(
            "brew_cask_dependency",
            "brew install sshfs",
            ""
        ));
    }

    #[test]
    fn brew_installs_the_formula_it_suggests() {
        let one = "Warning: No available formula with the name \"giss\". Did you mean gist?";
        let two = "Warning: No available formula with the name \"elasticserar\". Did you mean elasticsearch or elasticsearch@6?";
        let three =
            "Warning: No available formula with the name \"gitt\". Did you mean git, gitg or gist?";
        assert_eq!(
            fixes_of("brew_install", "brew install giss", one),
            ["brew install gist"]
        );
        assert_eq!(
            fixes_of("brew_install", "brew install elasticsear", two),
            ["brew install elasticsearch", "brew install elasticsearch@6"]
        );
        assert_eq!(
            fixes_of("brew_install", "brew install gitt", three),
            ["brew install git", "brew install gitg", "brew install gist"]
        );
        // newer brew says Error
        assert_eq!(
            fixes_of(
                "brew_install",
                "brew install giss",
                "Error: No available formula with the name \"giss\". Did you mean gist?"
            ),
            ["brew install gist"]
        );
        assert_eq!(brew_suggestions("one"), ["one"]);
        assert_eq!(brew_suggestions("one or two"), ["one", "two"]);
        assert_eq!(
            brew_suggestions("one, two or three"),
            ["one", "two", "three"]
        );
        assert!(!rule_matches(
            "brew_install",
            "brew install git",
            "Warning: git-2.3.5 already installed"
        ));
        assert!(!rule_matches(
            "brew_install",
            "brew install",
            "Install a formula or cask. Additional options specific to a formula may be"
        ));
    }

    #[test]
    fn brew_fixes_a_mistyped_command() {
        let fallback: Vec<String> = BREW_COMMANDS.iter().map(|name| name.to_string()).collect();
        let command = Command::new("brew inst", "Error: Unknown command: inst");
        assert_eq!(
            brew_unknown_command(&command, &fallback),
            ["brew list", "brew install", "brew uninstall"]
        );
        let fixes = brew_unknown_command(
            &Command::new("brew instaa", "Error: Unknown command: instaa"),
            &fallback,
        );
        assert!(fixes.contains(&"brew install".to_string()));
        assert!(fixes.contains(&"brew uninstall".to_string()));
        // newer brew repeats the program in the message, and calls it invalid usage
        assert_eq!(
            brew_unknown_command(
                &Command::new(
                    "brew isntall wget",
                    "Error: Invalid usage: Unknown command: brew isntall"
                ),
                &fallback
            )[0],
            "brew install wget"
        );
        assert_eq!(
            brew_unknown_command(
                &Command::new("brew instal", "Error: Unknown command: brew instal"),
                &fallback
            )[0],
            "brew install"
        );
        assert!(rule_matches(
            "brew_unknown_command",
            "brew inst",
            "Error: Unknown command: inst"
        ));
        assert!(!rule_matches("brew_unknown_command", "brew install", ""));
        assert!(!rule_matches(
            "brew_unknown_command",
            "git inst",
            "Error: Unknown command: inst"
        ));
    }

    #[test]
    fn brew_commands_come_from_its_checkout_and_taps() {
        // Intel: the checkout is <prefix>/Homebrew
        let intel = tempfile::tempdir().unwrap();
        let cmd = intel.path().join("Homebrew/Library/Homebrew/cmd");
        std::fs::create_dir_all(&cmd).unwrap();
        std::fs::write(cmd.join("install.rb"), "").unwrap();
        std::fs::write(cmd.join("list.sh"), "").unwrap();
        std::fs::write(cmd.join("README.md"), "").unwrap();
        let tap = intel
            .path()
            .join("Homebrew/Library/Taps/user/homebrew-extras/cmd");
        std::fs::create_dir_all(&tap).unwrap();
        std::fs::write(tap.join("brew-foo.rb"), "").unwrap();
        std::fs::write(tap.join("other.rb"), "").unwrap();
        assert_eq!(
            brew_commands_under(intel.path()),
            Some(vec![
                "foo".to_string(),
                "install".to_string(),
                "list".to_string()
            ])
        );

        // Apple silicon: the checkout is the prefix
        let arm = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(arm.path().join("Library/Homebrew/cmd")).unwrap();
        std::fs::write(arm.path().join("Library/Homebrew/cmd/doctor.rb"), "").unwrap();
        assert_eq!(
            brew_commands_under(arm.path()),
            Some(vec!["doctor".to_string()])
        );

        // no checkout: use the built-in list
        assert_eq!(
            brew_commands_under(tempfile::tempdir().unwrap().path()),
            None
        );
    }

    #[test]
    fn brew_upgrades_the_formula_not_brew() {
        let output = "Error: This command updates brew itself, and does not take formula names.\nUse `brew upgrade thefuck`.";
        assert_eq!(
            fixes_of("brew_update_formula", "brew update foo", output),
            ["brew upgrade foo"]
        );
        assert_eq!(
            fixes_of("brew_update_formula", "brew update bar zap", output),
            ["brew upgrade bar zap"]
        );
        assert!(!rule_matches("brew_update_formula", "brew upgrade foo", ""));
        assert!(!rule_matches("brew_update_formula", "brew update", ""));
    }

    #[test]
    fn choco_installs_the_install_package() {
        let output = "Chocolatey v0.10.15\nInstalling the following packages:\nlogstitcher\nBy installing you accept licenses for the packages.\nlogstitcher not installed. The package was not found with the source(s) listed.\n Source(s): 'https://chocolatey.org/api/v2/'\nChocolatey installed 0/1 packages. 1 packages failed.\n";
        for (script, fix) in [
            (
                "choco install logstitcher",
                "choco install logstitcher.install",
            ),
            ("cinst logstitcher", "cinst logstitcher.install"),
            (
                "choco install logstitcher -y",
                "choco install logstitcher.install -y",
            ),
            (
                "cinst logstitcher -y -n=test",
                "cinst logstitcher.install -y -n=test",
            ),
            (
                "choco install logstitcher -y -n=test /env",
                "choco install logstitcher.install -y -n=test /env",
            ),
            (
                "choco install chocolatey -y",
                "choco install chocolatey.install -y",
            ),
            ("cinst chocolatey -y", "cinst chocolatey.install -y"),
        ] {
            assert_eq!(fixes_of("choco_install", script, output), [fix], "{script}");
        }
        for script in [
            "choco /?",
            "choco upgrade logstitcher",
            "cup logstitcher",
            "choco uninstall logstitcher -y",
            "cuninst logstitcher",
        ] {
            assert!(!rule_matches("choco_install", script, ""), "{script}");
        }
    }

    #[test]
    fn dnf_fixes_a_mistyped_operation() {
        let invalid = |command: &str| {
            format!(
                "No such command: {command}. Please use /usr/bin/dnf --help\nIt could be a DNF plugin command, try: \"dnf install 'dnf-command({command})'\"\n"
            )
        };
        let operations = dnf_operations(DNF_HELP);
        assert_eq!(operations.len(), 40);
        assert_eq!(operations[..3], ["autoremove", "check", "check-update"]);
        assert_eq!(operations[39], "reposync");
        assert!(!operations.contains(&"usage".to_string()));
        for (script, output, fix) in [
            ("dnf isntall vim", invalid("isntall"), "dnf install vim"),
            ("dnf saerch vim", invalid("saerch"), "dnf search vim"),
        ] {
            let fixes = dnf_no_such_command(&Command::new(script, &output), Some(DNF_HELP));
            assert!(fixes.contains(&fix.to_string()), "{script}: {fixes:?}");
        }
        assert!(rule_matches(
            "dnf_no_such_command",
            "dnf",
            &invalid("saerch")
        ));
        assert!(rule_matches(
            "dnf_no_such_command",
            "sudo dnf isntall vim",
            &invalid("isntall")
        ));
        assert!(!rule_matches(
            "dnf_no_such_command",
            "pip",
            &invalid("isntall")
        ));
        assert!(!rule_matches("dnf_no_such_command", "vim", ""));
    }

    #[test]
    fn nixos_installs_what_it_suggests() {
        assert_eq!(
            fixes_of("nixos_cmd_not_found", "vim", "nix-env -iA nixos.vim"),
            ["nix-env -iA nixos.vim && vim"]
        );
        assert_eq!(
            fixes_of("nixos_cmd_not_found", "pacman", "nix-env -iA nixos.pacman"),
            ["nix-env -iA nixos.pacman && pacman"]
        );
        assert!(!rule_matches("nixos_cmd_not_found", "vim", ""));
    }

    const PYENV_COMMANDS: &str = "--version\nactivate\ncommands\ncompletions\ndeactivate\nexec_\nglobal\nhelp\nhooks\ninit\ninstall\nlocal\nprefix_\nrealpath.dylib\nrehash\nroot\nshell\nshims\nuninstall\nversion_\nversion-file\nversion-file-read\nversion-file-write\nversion-name_\nversion-origin\nversions\nvirtualenv\nvirtualenv-delete_\nvirtualenv-init\nvirtualenv-prefix\nvirtualenvs_\nvirtualenvwrapper\nvirtualenvwrapper_lazy\nwhence\nwhich_\n";

    #[test]
    fn omnienv_fixes_a_mistyped_command() {
        let output = |command: &str| format!("pyenv: no such command `{command}'");
        for (script, wrong, fix) in [
            ("pyenv globe", "globe", "pyenv global"),
            ("pyenv intall 3.8.0", "intall", "pyenv install 3.8.0"),
            ("pyenv list", "list", "pyenv install --list"),
            ("pyenv remove 3.8.0", "remove", "pyenv uninstall 3.8.0"),
        ] {
            let fixes = omnienv_no_such_command(
                &Command::new(script, &output(wrong)),
                Some(PYENV_COMMANDS),
            );
            assert!(fixes.contains(&fix.to_string()), "{script}: {fixes:?}");
            assert!(rule_matches(
                "omnienv_no_such_command",
                script,
                &output(wrong)
            ));
        }
        // the usual wrong guesses come first
        let list = omnienv_no_such_command(
            &Command::new("pyenv list", &output("list")),
            Some(PYENV_COMMANDS),
        );
        assert_eq!(list[..2], ["pyenv versions", "pyenv install --list"]);
        // goenv quotes it with '
        assert!(rule_matches(
            "omnienv_no_such_command",
            "goenv list",
            "goenv: no such command 'list'"
        ));
        let goenv = Command::new("goenv list", "goenv: no such command 'list'");
        assert_eq!(omnienv_no_such_command(&goenv, None)[0], "goenv versions");
        for (script, output) in [
            ("pyenv global", "system"),
            ("pyenv versions", "  3.7.0\n  3.7.1\n* 3.7.2\n"),
            ("pyenv install --list", "  3.7.0\n  3.7.1\n  3.7.2\n"),
        ] {
            assert!(!rule_matches("omnienv_no_such_command", script, output));
        }
    }

    const PKGFILE_SUDO: &str = "core/sudo 1.8.13-13/usr/bin/sudo";
    const PKGFILE_CONVERT: &str = "extra/imagemagick 6.9.1.0-1\t/usr/bin/convert";
    const PKGFILE_VIM: &str = "extra/gvim 7.4.712-1        \t/usr/bin/vim\nextra/gvim-python3 7.4.712-1\t/usr/bin/vim\nextra/vim 7.4.712-1         \t/usr/bin/vim\nextra/vim-minimal 7.4.712-1 \t/usr/bin/vim\nextra/vim-python3 7.4.712-1 \t/usr/bin/vim";

    #[test]
    fn pacman_installs_the_package_with_the_program() {
        assert_eq!(
            parse_pkgfile(PKGFILE_VIM),
            [
                "extra/gvim",
                "extra/gvim-python3",
                "extra/vim",
                "extra/vim-minimal",
                "extra/vim-python3"
            ]
        );
        assert_eq!(parse_pkgfile(PKGFILE_SUDO), ["core/sudo"]);
        assert_eq!(
            pacman_install_fixes(
                &Command::new("sudo vim", ""),
                "yay",
                &parse_pkgfile(PKGFILE_VIM)
            ),
            [
                "yay -S extra/gvim && sudo vim",
                "yay -S extra/gvim-python3 && sudo vim",
                "yay -S extra/vim && sudo vim",
                "yay -S extra/vim-minimal && sudo vim",
                "yay -S extra/vim-python3 && sudo vim"
            ]
        );
        assert_eq!(
            pacman_install_fixes(
                &Command::new("convert", ""),
                "sudo pacman",
                &parse_pkgfile(PKGFILE_CONVERT)
            ),
            ["sudo pacman -S extra/imagemagick && convert"]
        );
        assert!(parse_pkgfile("").is_empty());
        // nothing is asked of pkgfile for a flag
        assert!(pkgfile_packages("--version").is_empty());
        // no "not found", no rule
        assert!(!rule_matches("pacman", "vim", ""));
        assert!(!rule_matches("pacman", "sudo vim", ""));
    }

    #[test]
    fn pacman_capitalizes_the_operation() {
        let bad = "error: invalid option '-";
        for option in "surqfdvt".chars() {
            let script = format!("pacman -{option}v meat");
            let upper = option.to_ascii_uppercase();
            assert_eq!(
                fixes_of("pacman_invalid_option", &script, bad),
                [format!("pacman -{upper}v meat")]
            );
        }
        assert_eq!(
            fixes_of("pacman_invalid_option", "sudo pacman -syu", bad),
            ["sudo pacman -Syu"]
        );
        let good = "community/shared_meataxe 1.0-3\n    A set of programs for working with matrix representations over finite fields\n";
        for option in "SURQFDVT".chars() {
            assert!(!rule_matches(
                "pacman_invalid_option",
                &format!("pacman -{option}s meat"),
                good
            ));
        }
        for option in "azxcbnm".chars() {
            assert!(!rule_matches(
                "pacman_invalid_option",
                &format!("pacman -{option}v meat"),
                bad
            ));
        }
    }

    #[test]
    fn pacman_fixes_a_package_name() {
        let output = "error: target not found: llc";
        let packages = parse_pkgfile(
            "extra/llvm 3.6.0-5      /usr/bin/llc\nextra/llvm35 3.5.2-13/usr/bin/llc",
        );
        for (script, fixes) in [
            ("yay -S llc", ["yay -S extra/llvm", "yay -S extra/llvm35"]),
            (
                "pikaur -S llc",
                ["pikaur -S extra/llvm", "pikaur -S extra/llvm35"],
            ),
            (
                "yaourt -S llc",
                ["yaourt -S extra/llvm", "yaourt -S extra/llvm35"],
            ),
            (
                "pacman -S llc",
                ["pacman -S extra/llvm", "pacman -S extra/llvm35"],
            ),
            (
                "sudo pacman -S llc",
                ["sudo pacman -S extra/llvm", "sudo pacman -S extra/llvm35"],
            ),
        ] {
            assert_eq!(
                pacman_not_found(&Command::new(script, output), &packages),
                fixes
            );
            assert!(rule_matches("pacman_not_found", script, output), "{script}");
        }
        assert!(rule_matches("pacman_not_found", "pacman llc", output));
        assert!(rule_matches("pacman_not_found", "sudo pacman llc", output));
        assert!(!rule_matches("pacman_not_found", "pacman -S llvm", ""));
        assert!(!rule_matches("pacman_not_found", "apt install llc", output));
    }

    #[test]
    fn yum_fixes_a_mistyped_operation() {
        let invalid = |command: &str| {
            format!(
                "Loaded plugins: extras_suggestions, langpacks, priorities, update-motd\nNo such command: {command}. Please use /usr/bin/yum --help\n"
            )
        };
        let operations = yum_operations(YUM_HELP);
        assert_eq!(operations.len(), 34);
        assert_eq!(operations[..3], ["check", "check-update", "clean"]);
        assert!(operations.contains(&"distribution-synchronization".to_string()));
        assert_eq!(operations[33], "version");
        let first = |script: &str, wrong: &str| {
            yum_invalid_operation(&Command::new(script, &invalid(wrong)), Some(YUM_HELP))[0].clone()
        };
        assert_eq!(first("yum uninstall", "uninstall"), "yum remove");
        assert_eq!(first("yum saerch asdf", "saerch"), "yum search asdf");
        assert_eq!(first("yum hlep", "hlep"), "yum help");
        assert_eq!(
            with_sudo(
                &Command::new("sudo yum saerch asdf", &invalid("saerch")),
                |c| { yum_invalid_operation(c, Some(YUM_HELP)) }
            )[0],
            "sudo yum search asdf"
        );
        for command in ["saerch", "uninstall"] {
            assert!(rule_matches(
                "yum_invalid_operation",
                &format!("yum {command}"),
                &invalid(command)
            ));
        }
        let search = "Warning: No matches found for: asdf\nNo matches found\n";
        for (script, output) in [
            ("vim", ""),
            ("yum help", YUM_HELP),
            ("yum search asdf", search),
        ] {
            assert!(
                !rule_matches("yum_invalid_operation", script, output),
                "{script}"
            );
        }
    }
}

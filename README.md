<p align="center"><img src="docs/logo.svg" alt="welp" width="420"></p>

<p align="center"><b>Typed it wrong? Type <code>welp</code>.</b><br>
It fixes the command you just messed up, shrugs, and moves on.<br>
A drop-in replacement for <a href="https://github.com/nvbn/thefuck">thefuck</a>: every rule, every shell, no Python, and one command to switch.</p>

<p align="center">
<a href="https://crates.io/crates/welp"><img src="https://img.shields.io/crates/v/welp?color=FFCC4D&labelColor=141414" alt="crates.io"></a>
<a href="https://buymeacoffee.com/oliv3rmoon"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-FFDD00?style=flat&logo=buymeacoffee&logoColor=black" alt="Buy me a coffee"></a>
<a href="https://discord.gg/pSqgRU3YP9"><img src="https://img.shields.io/badge/Discord-The%20Commons-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord"></a>
</p>

<p align="center"><img src="docs/demo.gif" alt="welp fixing gti into git status, picking git push out of two suggestions with the arrow keys, setting the upstream, fixing cd.., shrugging at a command with no fix, then showing welp stats" width="900"></p>

```console
project ❯ gti status
zsh: command not found: gti
project ❯ welp
 welp  git status  ↵ run · esc nah
```

## Why welp

thefuck is one of the most-loved command-line tools ever made. It has also had no release since January 2022, and it breaks on Python 3.12 and newer (`No module named 'imp'`). welp keeps what made it great and quietly fixes the rest.

| | thefuck | welp |
|---|---|---|
| Needs Python | yes | no, one small binary |
| Last release | January 2022 | active |
| Speed | starts Python every time | starts in about 3 ms, a typical fix in about 20 ms |
| Runs a failed command again to read its output | every command | never `rm`, `mv`, `dd`, editors, `ssh` and the like |
| `fuck --yeah` | yes | yes |
| Moves your setup over | | `welp migrate` keeps your alias and settings |
| Your own rules | Python files | TOML files, no code needed |
| Rules | 168 | all 168, same names, plus `did_you_mean`; the 7 that can destroy something are opt-in |
| Personality | a memorable name | a shrug mascot, three voices, milestones, `welp stats` |
| Shells | bash, zsh, fish, tcsh, PowerShell | bash, zsh, fish, tcsh, PowerShell |
| Install | `pip` | `brew`, `cargo`, or a download for macOS, Linux and Windows |

Speed measured on a MacBook Pro with `welp fix`; your numbers will vary.

### Everything thefuck does, welp does

- [x] All 168 rules, under the same names, so `exclude_rules` and `rules` from your thefuck settings keep working
- [x] bash, zsh, fish, tcsh and PowerShell, each tested in CI by mistyping a command and running `fuck --yeah`
- [x] `fuck`, `fuck --yeah` / `-y`, and picking between fixes with the arrow keys
- [x] macOS, Linux and Windows
- [x] Your own rules (TOML here instead of Python)
- [x] Your settings: `welp migrate` moves `settings.py`, your alias and your shell config over in one go

## Meet the voice

welp is lowercase, low-key and a little dry. It shrugs when it has nothing, has a line for particular fixes, marks milestones and notices when you keep making the same typo:

```console
project ❯ welp
 welp  git status
        gti → git, 10 times now. we don't have to talk about it.

project ❯ welp
 welp  ¯\_(ツ)_/¯  nothing to fix here. that one's all you.
```

Pick a voice in your settings: `chill` (the default, a line now and then), `spicy` (teases you every time) or `quiet` (just the fix).

`welp stats` shows what it remembers:

```console
project ❯ welp stats
 welp  your mistakes, lovingly kept

        fixes         142 since oct 22, 2026
        favorite rule git_push (38)
        top typos     gti → git (14) · sl → ls (5) · pyhton → python (3)
        verdict       you and git have a thing going on.
```

It keeps only counts, rule names and the mistyped words, never your commands, in `~/.local/state/welp/stats.toml`. Set `remember = false` to turn it off.

## Install

```sh
brew install oliv3rmoon/tap/welp    # macOS and Linux
cargo install welp                   # anywhere with Rust
```

Or grab a prebuilt binary for macOS, Linux or Windows from the [releases page](https://github.com/Oliv3rmoon/welp/releases/latest), unpack it and put `welp` somewhere on your `PATH`.

## Set up

Add one line to your shell config, then open a new terminal:

| Shell | Line | File |
|---|---|---|
| zsh | `eval "$(welp init zsh)"` | `~/.zshrc` |
| bash | `eval "$(welp init bash)"` | `~/.bashrc` |
| fish | `welp init fish \| source` | `~/.config/fish/config.fish` |
| tcsh | ``eval `welp init tcsh` `` | `~/.tcshrc` |
| PowerShell | `Invoke-Expression (& welp init powershell \| Out-String)` | your `$PROFILE` |

Prefer another word? Add `--alias`, for example `eval "$(welp init zsh --alias oops)"`.

### Coming from thefuck

```sh
welp migrate --dry-run   # see what would change
welp migrate             # do it
```

It comments out the thefuck lines in your shell config (bash, zsh, fish, tcsh or PowerShell, with a backup next to each file), adds welp in their place with your alias, and turns `~/.config/thefuck/settings.py` into `~/.config/welp/config.toml`, including your `exclude_rules` and any extra `rules` you turned on.

## Rules

welp has all of thefuck's rules (168 of them, with the same names) plus `did_you_mean`, which uses the fix a tool suggests itself. The everyday ones:

| Rule | What it fixes |
|---|---|
| `no_command` | A mistyped program name: `gti` → `git` |
| `git_not_command` | A mistyped git subcommand: `git comit` → `git commit` |
| `git_push` | Sets the upstream when a new branch has none |
| `git_stash` | Stashes your changes when a checkout or pull needs a clean tree |
| `sudo` | Runs the command again with sudo after "permission denied" |
| `cd_correction` | `cd projcts` → `cd projects` |
| `mkdir_p` | Adds `-p` when parent folders are missing |
| `did_you_mean` | Uses the fix a tool suggests: cargo, npm, pip, brew, gh and more |

Seven rules are off by default because their fix deletes, force-pushes, kills or removes something the typed command didn't ask to: `git_push_force`, `git_tag_force`, `port_already_in_use`, `rm_root`, `ssh_known_hosts`, `django_south_ghost` and `docker_image_being_used_by_container`. Turn one on with `enable_rules`. `welp rules` lists every rule and which are off.

<details>
<summary>All 169 rules</summary>

| Rule | What it fixes |
|---|---|
| `adb_unknown_command` | Fixes a mistyped adb command |
| `ag_literal` | Searches ag for a literal string with -Q after a bad regex |
| `apt_get` | Installs the missing program the system names, then runs it |
| `apt_get_search` | Searches with apt-cache, since apt-get has no search |
| `apt_invalid_operation` | Fixes a mistyped apt operation |
| `apt_list_upgradable` | Lists the upgradable packages apt mentions |
| `apt_upgrade` | Upgrades after listing the upgradable packages |
| `aws_cli` | Takes the aws command the cli suggests |
| `az_cli` | Takes the az command the cli suggests |
| `brew_cask_dependency` | Installs the cask a formula needs first |
| `brew_install` | Installs the formula brew suggests |
| `brew_link` | Previews what `brew link --overwrite` would replace |
| `brew_reinstall` | Reinstalls a formula that is already installed and up to date |
| `brew_uninstall` | Adds --force to remove every installed version |
| `brew_unknown_command` | Fixes a mistyped brew command |
| `brew_update_formula` | Upgrades the formula, since brew update only updates brew |
| `cargo` | Builds when cargo is run without a command |
| `cargo_no_command` | Takes the cargo command it suggests |
| `cat_dir` | Lists a folder instead of trying to cat it |
| `cd_correction` | Fixes a misspelled folder in a cd path |
| `cd_cs` | cs to cd, the keys sit side by side |
| `cd_mkdir` | Creates the folder you tried to cd into |
| `cd_parent` | cd.. to cd .. |
| `chmod_x` | Makes a script executable before running it |
| `choco_install` | Installs the .install package of the same name |
| `composer_not_command` | Takes the composer command it suggests |
| `conda_mistype` | Takes the conda command it suggests |
| `cp_create_destination` | Creates the destination folder, then copies or moves into it |
| `cp_omitting_directory` | Adds -r to copy a folder |
| `cpp11` | Adds -std=c++11 when the compiler needs it |
| `did_you_mean` | Uses the fix a tool suggests (cargo, npm, pip, brew, gh and more) |
| `dirty_untar` | Extracts a tar archive into a folder of its own |
| `dirty_unzip` | Extracts a zip with many files into a folder of its own |
| `django_south_ghost` | Deletes ghost migrations from the database with --delete-ghost-migrations (off by default) |
| `django_south_merge` | Adds --merge to a South migration that ran out of order |
| `dnf_no_such_command` | Fixes a mistyped dnf command |
| `docker_image_being_used_by_container` | Force-removes the container that uses the image, then removes it (off by default) |
| `docker_login` | Logs in to docker first |
| `docker_not_command` | Fixes a mistyped docker command |
| `dry` | Drops a word you typed twice |
| `fab_command_not_found` | Fixes a mistyped fabric task |
| `fix_alt_space` | Swaps a non-breaking space (Alt+Space) for a normal one |
| `fix_file` | Opens $EDITOR at the line an error points to, then runs the command again |
| `gem_unknown_command` | Fixes a mistyped gem command |
| `git_add` | Stages a file git does not know yet, then runs the command again |
| `git_add_force` | Adds --force to stage a file your .gitignore covers |
| `git_bisect_usage` | Uses the git bisect subcommand that looks like the one you typed |
| `git_branch_0flag` | Turns 0a, 0d, 0v and friends into -a, -d, -v after git branch |
| `git_branch_delete` | Force-deletes a branch that is not merged |
| `git_branch_delete_checked_out` | Leaves the branch, then deletes it |
| `git_branch_exists` | Recreates or switches to a branch that already exists |
| `git_branch_list` | Deletes the branch called list that `git branch list` made, then lists branches |
| `git_checkout` | Creates the branch when it does not exist yet |
| `git_clone_git_clone` | Drops the second `git clone` from a pasted clone command |
| `git_clone_missing` | Adds git clone in front of a pasted repository address |
| `git_commit_add` | Stages changed files when a commit has nothing staged |
| `git_commit_amend` | Amends the commit you just made |
| `git_commit_reset` | Undoes the last commit and keeps its changes |
| `git_diff_no_index` | Adds --no-index to diff two files git does not track |
| `git_diff_staged` | Adds --staged to diff what is staged |
| `git_fix_stash` | Fixes a misspelled git stash subcommand, or saves with a message |
| `git_flag_after_filename` | Moves a flag in front of the file name |
| `git_help_aliased` | Opens help for the command an alias stands for |
| `git_hook_bypass` | Adds --no-verify to skip git hooks on am, commit or push |
| `git_lfs_mistype` | Uses the git lfs command git-lfs suggests |
| `git_main_master` | Swaps main and master when the branch is not there |
| `git_merge` | Merges the remote branch git suggests |
| `git_merge_unrelated` | Adds --allow-unrelated-histories to a merge git refuses |
| `git_not_command` | Fixes a mistyped git subcommand |
| `git_pull` | Sets the tracking branch, then pulls |
| `git_pull_clone` | Clones the repository you tried to pull outside of any repository |
| `git_pull_uncommitted_changes` | Stashes your changes around the pull |
| `git_push` | Sets the upstream when a new branch has none |
| `git_push_different_branch_names` | Pushes the way git says to when the branch names differ |
| `git_push_force` | Pushes with --force-with-lease when the remote is ahead (off by default) |
| `git_push_pull` | Pulls first when a push is rejected, then pushes |
| `git_push_without_commits` | Makes the first commit, then pushes |
| `git_rebase_merge_dir` | Continues, skips or aborts the rebase that is already going |
| `git_rebase_no_changes` | Skips the patch a rebase finds empty |
| `git_remote_delete` | git remote remove, not delete |
| `git_remote_seturl_add` | Adds the remote when set-url finds none by that name |
| `git_rm_local_modifications` | Adds --cached to keep a file with local changes |
| `git_rm_recursive` | Adds -r to remove a folder |
| `git_rm_staged` | Adds --cached to keep a file with staged changes |
| `git_stash` | Stashes your changes, then runs the command again |
| `git_stash_pop` | Stages the changes that block git stash pop, pops, then unstages |
| `git_tag_force` | Adds --force to move a tag that already exists (off by default) |
| `git_two_dashes` | Puts the second dash on a long option, as git asks |
| `go_run` | Adds .go to the file `go run` was given |
| `go_unknown_command` | Fixes a mistyped go command |
| `gradle_no_task` | Fixes a mistyped gradle task |
| `gradle_wrapper` | Uses ./gradlew when gradle itself is not installed |
| `grep_arguments_order` | Puts the file after the pattern in grep |
| `grep_recursive` | Adds -r to search a folder |
| `grunt_task_not_found` | Fixes a mistyped grunt task |
| `gulp_not_task` | Fixes a mistyped gulp task |
| `has_exists_script` | Runs a script in this folder with ./ |
| `heroku_multiple_apps` | Names one of the apps in the git remotes with --app |
| `heroku_not_command` | Runs the command heroku says it meant |
| `history` | Tries the closest command from your history |
| `hostscli` | Fixes a misspelled hostscli command, or lists the websites it knows |
| `ifconfig_device_not_found` | Uses an interface ifconfig lists instead of the one you typed |
| `java` | Drops .java when running a class |
| `javac` | Adds .java to the file javac was given |
| `lein_not_task` | Takes the lein task it suggests |
| `ln_no_hard_link` | Adds -s when a hard link to a folder is not allowed |
| `ln_s_order` | Puts the link name last in ln -s |
| `long_form_help` | Uses --help, or the help command the tool suggests |
| `ls_all` | Adds -A when ls shows nothing, to see hidden files |
| `ls_lah` | Lists with -lah |
| `man` | Tries another man section, or the --help of the tool |
| `man_no_space` | Adds the missing space after man |
| `mercurial` | Uses the hg command hg suggests |
| `missing_space_before_subcommand` | Adds the missing space after a program name: gitbranch to git branch |
| `mkdir_p` | Adds -p when parent folders are missing |
| `mvn_no_command` | Gives mvn a goal to run |
| `mvn_unknown_lifecycle_phase` | Fixes a mistyped mvn lifecycle phase |
| `nixos_cmd_not_found` | Installs the program with nix-env, as NixOS suggests, then runs it |
| `no_command` | Fixes a mistyped program name |
| `no_such_file` | Creates the folder cp or mv could not find, then runs the command again |
| `npm_missing_script` | Fixes a mistyped script name for npm run |
| `npm_run_script` | Uses npm run-script for a script from package.json |
| `npm_wrong_command` | Fixes a mistyped npm command |
| `omnienv_no_such_command` | Fixes a mistyped goenv, nodenv, pyenv or rbenv command |
| `open` | Opens a web address with http://, or creates the missing file or folder |
| `pacman` | Installs the package that has the missing program, then runs it |
| `pacman_invalid_option` | Capitalizes pacman's operation flag: -s to -S |
| `pacman_not_found` | Fixes a package name pacman does not know |
| `path_from_history` | Swaps a path that is not there for a longer one from your history |
| `php_s` | Uses -S, the built-in server flag, in place of -s |
| `pip_install` | Installs with --user (then sudo) after a permission error |
| `pip_unknown_command` | Takes the pip command it suggests |
| `port_already_in_use` | Kills the process that holds the port, then runs the command again (off by default) |
| `prove_recursively` | Adds -r to prove to look inside folders |
| `python_command` | Runs a .py script with python |
| `python_execute` | Adds .py to the script python was given |
| `python_module_error` | Installs the missing Python module with pip, then runs the command |
| `quotation_marks` | Makes mismatched quotes match: 'text" to "text" |
| `rails_migrations_pending` | Runs the pending migrations rails asks for, then the command |
| `react_native_command_unrecognized` | Fixes a mistyped react-native command |
| `remove_shell_prompt_literal` | Drops the $ copied from a documentation code block |
| `remove_trailing_cedilla` | Drops a ç typed at the end |
| `rm_dir` | Adds -r to remove a folder |
| `rm_root` | Adds --no-preserve-root to rm on / (off by default) |
| `scm_correction` | Uses git or hg, whichever repository this folder holds |
| `sed_unterminated_s` | Closes the s/// command with a slash |
| `sl_ls` | sl to ls |
| `ssh_known_hosts` | Removes the changed host key with ssh-keygen -R, then connects again (off by default) |
| `sudo` | Runs the command again with sudo |
| `sudo_command_from_user_path` | Keeps your PATH under sudo for a program only you can find |
| `switch_lang` | Retypes a command written in another keyboard layout |
| `systemctl` | Puts the action before the service in systemctl |
| `terraform_init` | Runs terraform init first |
| `terraform_no_command` | Takes the terraform command it suggests |
| `tmux` | Picks the full tmux command out of the ones it says it could be |
| `touch` | Creates the missing folders, then the file |
| `tsuru_login` | Logs in to tsuru first |
| `tsuru_not_command` | Takes the tsuru command it suggests |
| `unknown_command` | Uses the command a tool says you probably meant (hdfs and others) |
| `unsudo` | Drops sudo from a command that refuses to run as root |
| `vagrant_up` | Starts the machine with vagrant up first |
| `whois` | Asks whois about the domain instead of the whole address |
| `workon_doesnt_exists` | Fixes a mistyped virtualenv name, or creates it with mkvirtualenv |
| `wrong_hyphen_before_subcommand` | Swaps the hyphen in git-log style commands for a space |
| `yarn_alias` | Takes the yarn command it suggests |
| `yarn_command_not_found` | Fixes a mistyped yarn command |
| `yarn_command_replaced` | Runs the command yarn says replaced the old one |
| `yarn_help` | Opens the documentation page yarn help points to |
| `yum_invalid_operation` | Fixes a mistyped yum operation |

</details>

## Settings

`~/.config/welp/config.toml` (or `$XDG_CONFIG_HOME/welp/config.toml`):

```toml
require_confirmation = true   # false runs the first fix straight away
wait_command = 3              # seconds to wait when reading a command's output
wait_slow_command = 15
slow_commands = ["lein", "react-native", "gradle", "./gradlew", "vagrant"]
no_rerun = ["rm", "mv", "dd", "vim", "ssh"]  # never run these twice (the full default list is longer)
exclude_rules = []
enable_rules = []             # opt into the 7 rules that are off by default
voice = "chill"               # chill, spicy or quiet
remember = true               # false stops welp stats from counting
```

Set `NO_COLOR` to drop the colours; welp keeps the words.

## Your own rules

Teach welp a fix without writing code: drop a TOML file in `~/.config/welp/rules/`.

```toml
# ~/.config/welp/rules/please.toml
name = "please"
description = "please → sudo"
script = '^please (?P<rest>.+)$'
fix = "sudo {rest}"
```

A rule can check the `program`, a regex on the command (`script`) and a regex on what it printed (`output`). Its `fix` (or a list of `fixes`) can use `{script}` and any named group from those regexes. Your fixes are offered first; add `first = false` to offer them after welp's own. `welp rules` lists yours and says what's wrong with any file it can't read.

## Adding a rule to welp

A rule is a name, a check and a fix, in `src/rules/`. Each comes with a test that feeds it a command and the output a real tool printed:

```rust
Rule {
    name: "mkdir_p",
    description: "Adds -p when parent folders are missing",
    priority: 1000,
    matches: |c| c.is("mkdir") && c.output_lower().contains("no such file or directory"),
    fixes: |c| vec![c.replace_word("mkdir", "mkdir -p")],
},
```

Run `cargo test` before opening a pull request.

The demo above is a real session recorded in Ghostty: `demo/record.sh` sets up a throwaway repo and types the commands into zsh. There is also an [MP4 version](docs/demo.mp4).

## Support

If welp saves you some typing, you can [buy me a coffee](https://buymeacoffee.com/oliv3rmoon) or come hang out on [Discord](https://discord.gg/pSqgRU3YP9).

## Credits

welp's rules are ports of [thefuck](https://github.com/nvbn/thefuck)'s, by Vladimir Iakovlev and its contributors (MIT). welp is not affiliated with thefuck.

## License

MIT

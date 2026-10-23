<h1 align="center">welp</h1>

<p align="center"><b>Typed it wrong? Type <code>welp</code>.</b><br>
It fixes the command you just messed up, and doesn't make a big deal about it.<br>
A fast, single-binary successor to <a href="https://github.com/nvbn/thefuck">thefuck</a>, with a one-command move from it.</p>

<p align="center">
<a href="https://buymeacoffee.com/oliv3rmoon"><img src="https://img.shields.io/badge/Buy%20me%20a%20coffee-FFDD00?style=flat&logo=buymeacoffee&logoColor=black" alt="Buy me a coffee"></a>
<a href="https://discord.gg/pSqgRU3YP9"><img src="https://img.shields.io/badge/Discord-The%20Commons-5865F2?style=flat&logo=discord&logoColor=white" alt="Discord"></a>
</p>

<p align="center"><img src="docs/demo.gif" alt="welp fixing a mistyped command, a git push with no upstream, a mkdir with missing folders and cd.." width="900"></p>

```console
project ❯ gti status
zsh: command not found: gti
project ❯ welp
 welp  git status  ↵ run · esc nah
```

## Why welp

thefuck is one of the most-loved command-line tools ever made. It has also had no release since January 2022, and it breaks on Python 3.12 and newer (`No module named 'imp'`). welp keeps what made it great and quietly fixes the rest:

- **No Python.** One small binary that starts instantly, on any Python version or none.
- **Safe.** welp never runs destructive or interactive commands (`rm`, `mv`, `dd`, editors, `ssh`, ...) a second time just to read their output. Rules that need to know more look at your files instead.
- **Moves your setup for you.** `welp migrate` switches your shell config, keeps your alias (yes, `fuck` still works) and imports your settings.
- **Same rule names.** Your thefuck `exclude_rules` keep working.
- **Workplace-friendly name.** Use whatever alias you like.
- **A little personality.** Lowercase, low-key, and the occasional "happens to the best of us." Set `quiet = true` if you'd rather it just did its job.

## Install

```sh
cargo install welp
```

Homebrew and prebuilt binaries are coming.

## Set up

Add one line to your shell config, then open a new terminal:

| Shell | Line | File |
|---|---|---|
| zsh | `eval "$(welp init zsh)"` | `~/.zshrc` |
| bash | `eval "$(welp init bash)"` | `~/.bashrc` |
| fish | `welp init fish \| source` | `~/.config/fish/config.fish` |

Prefer another word? Add `--alias`, for example `eval "$(welp init zsh --alias oops)"`.

### Coming from thefuck

```sh
welp migrate --dry-run   # see what would change
welp migrate             # do it
```

It comments out the thefuck lines in your shell config (with a backup next to each file), adds welp in their place with your alias, and turns `~/.config/thefuck/settings.py` into `~/.config/welp/config.toml`.

## Rules

| Rule | What it fixes |
|---|---|
| `no_command` | A mistyped program name: `gti` → `git` |
| `git_not_command` | A mistyped git subcommand: `git comit` → `git commit` |
| `git_push` | Sets the upstream when a new branch has none |
| `git_pull` | Sets the tracking branch, then pulls |
| `git_checkout` | Creates the branch when it does not exist yet |
| `git_branch_delete` | `-d` → `-D` for a branch that is not merged |
| `git_commit_add` | Stages changed files when a commit has nothing staged |
| `git_add` | Stages a file git does not know yet, then runs the command again |
| `git_add_force` | Adds `--force` to stage a file your `.gitignore` covers |
| `git_branch_exists` | Recreates or switches to a branch that already exists (`-d`, never `-D`) |
| `git_clone_git_clone` | Drops the second `git clone` from a pasted command |
| `git_hook_bypass` | Adds `--no-verify` to skip hooks on `am`, `commit` or `push` |
| `git_main_master` | Swaps `main` and `master` when the branch is not there |
| `git_merge` | Merges the remote branch git suggests |
| `git_push_pull` | Pulls first when a push is rejected, then pushes |
| `git_rebase_no_changes` | `git rebase --skip` for a patch that came out empty |
| `git_remote_seturl_add` | `set-url` → `add` when the remote does not exist |
| `git_rm_local_modifications` | Adds `--cached` to keep a file with local changes |
| `git_rm_recursive` | Adds `-r` to remove a folder |
| `git_rm_staged` | Adds `--cached` to keep a file with staged changes |
| `git_stash` | Stashes your changes, then runs the command again |
| `did_you_mean` | Uses the fix a tool suggests: cargo, npm, pip, brew, gh and more |
| `sudo` | Runs the command again with sudo after "permission denied" |
| `chmod_x` | Makes a script executable before running it |
| `apt_get` | Installs a missing program the system names, then runs it |
| `cd_mkdir` | Creates the folder you tried to `cd` into |
| `cd_parent` | `cd..` → `cd ..` |
| `mkdir_p` | Adds `-p` when parent folders are missing |
| `cp_omitting_directory` | Adds `-r` to copy a folder |
| `rm_dir` | Adds `-r` to remove a folder |
| `grep_recursive` | Adds `-r` to search a folder |
| `touch` | Creates missing folders, then the file |
| `sl_ls` | `sl` → `ls` |
| `cat_dir` | `cat` on a folder → `ls` |
| `cd_correction` | Fixes a misspelled folder in a `cd` path: `cd Documnets` → `cd Documents` |
| `has_exists_script` | Runs a script in this folder with `./` |
| `ln_no_hard_link` | Adds `-s` when a hard link to a folder is not allowed |
| `man_no_space` | `mandiff` → `man diff` |
| `missing_space_before_subcommand` | `gitbranch` → `git branch` |
| `quotation_marks` | Makes mismatched quotes match: `'text"` → `"text"` |
| `sudo_command_from_user_path` | Keeps your `PATH` under `sudo` for a program only you can find |
| `unsudo` | Drops `sudo` from a command that refuses to run as root |
| `brew_link` | Previews what `brew link --overwrite` would replace |
| `brew_reinstall` | Reinstalls a formula that is already installed and up to date |
| `brew_uninstall` | Adds `--force` to remove every installed version |
| `go_run` | `go run foo` → `go run foo.go` |
| `java` | `java Foo.java` → `java Foo` |
| `javac` | `javac Foo` → `javac Foo.java` |
| `npm_run_script` | `npm build` → `npm run-script build` for a script in `package.json` |
| `pip_install` | Installs with `--user` (then `sudo`) after a permission error |
| `python_execute` | `python foo` → `python foo.py` |
| `yarn_command_not_found` | Fixes a mistyped yarn command |

`welp rules` lists them. More of thefuck's 170 rules are being ported. Pull requests are welcome.

## Settings

`~/.config/welp/config.toml` (or `$XDG_CONFIG_HOME/welp/config.toml`):

```toml
require_confirmation = true   # false runs the first fix straight away
wait_command = 3              # seconds to wait when reading a command's output
wait_slow_command = 15
slow_commands = ["lein", "react-native", "gradle", "./gradlew", "vagrant"]
no_rerun = ["rm", "mv", "dd", "vim", "ssh"]  # never run these twice (the full default list is longer)
exclude_rules = []
quiet = false                 # true keeps the occasional one-liners to itself
```

Set `NO_COLOR` to drop the colours; welp keeps the words.

## Adding a rule

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

The demo above is a real session: `demo/record.sh` sets up a throwaway repo and types the commands into zsh.

## Support

If welp saves you some typing, you can [buy me a coffee](https://buymeacoffee.com/oliv3rmoon) or come hang out on [Discord](https://discord.gg/pSqgRU3YP9).

## Credits

welp's rules are ports of [thefuck](https://github.com/nvbn/thefuck)'s, by Vladimir Iakovlev and its contributors (MIT). welp is not affiliated with thefuck.

## License

MIT

#!/bin/sh
# Builds the throwaway project the demo runs in: a repo on a new branch with
# no upstream, and a local "origin" to push to.
set -e
root="$1"
rm -rf "$root" && mkdir -p "$root/zdotdir"
git init -q --bare "$root/origin.git"
git init -q -b main "$root/project"
cd "$root/project"
git config user.name demo && git config user.email demo@example.com
echo "# project" > README.md && git add README.md && git commit -qm "first commit"
git remote add origin "$root/origin.git" && git push -q -u origin main
git switch -qc feature
cat > "$root/zdotdir/.zshrc" <<'RC'
PROMPT='%F{cyan}%1~%f %F{green}❯%f '
HISTSIZE=100
unsetopt beep
PROMPT_EOL_MARK=''
eval "$(welp init zsh)"
RC
# A lived-in history for welp stats, so the first fix in the demo is the 100th.
mkdir -p "$root/state/welp"
cat > "$root/state/welp/stats.toml" <<'STATS'
total = 99
since = 1788566400

[rules]
git_push = 31
no_command = 22
git_not_command = 14
sudo = 9
cd_parent = 6
mkdir_p = 5
sl_ls = 5
did_you_mean = 4
git_stash = 3

[typos]
"gti → git" = 12
"sl → ls" = 5
"pyhton → python" = 3
"pul → push" = 2
STATS

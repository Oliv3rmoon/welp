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

use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum Shell {
    Bash,
    Zsh,
    Fish,
}

impl Shell {
    /// The line that goes in the shell's config file.
    pub fn init_line(self, alias: &str) -> String {
        let alias = if alias == "welp" {
            String::new()
        } else {
            format!(" --alias {alias}")
        };
        match self {
            Shell::Bash => format!("eval \"$(welp init bash{alias})\""),
            Shell::Zsh => format!("eval \"$(welp init zsh{alias})\""),
            Shell::Fish => format!("welp init fish{alias} | source"),
        }
    }

    /// The function the shell runs when you type the alias. `-y`, `--yes`
    /// or thefuck's `--yeah` run the first fix without asking; any other
    /// arguments make it plain `welp`, so `welp init` and friends still work.
    pub fn script(self, alias: &str) -> String {
        match self {
            Shell::Bash => format!(
                r#"{alias}() {{
    local yes=
    case "$1" in -y|--yes|--yeah) yes=--yes; shift ;; esac
    if [ "$#" -gt 0 ]; then command welp "$@"; return; fi
    local fix
    fix=$(WELP_ALIAS={alias} WELP_HISTORY="$(fc -ln -10)" command welp fix $yes) || return
    [ -n "$fix" ] || return
    history -s "$fix"
    eval "$fix"
}}
"#
            ),
            Shell::Zsh => format!(
                r#"{alias}() {{
    local yes=
    case "$1" in -y|--yes|--yeah) yes=--yes; shift ;; esac
    if (( $# > 0 )); then command welp "$@"; return; fi
    local fix
    fix=$(WELP_ALIAS={alias} WELP_HISTORY="$(fc -ln -10)" command welp fix $yes) || return
    [[ -n $fix ]] || return
    print -s -- "$fix"
    eval "$fix"
}}
"#
            ),
            Shell::Fish => format!(
                r#"function {alias} --description 'Fix your last command'
    set -l yes
    if contains -- "$argv[1]" -y --yes --yeah
        set yes --yes
        set -e argv[1]
    end
    if test (count $argv) -gt 0
        command welp $argv
        return
    end
    set -l fix (env WELP_ALIAS={alias} WELP_NEWEST_FIRST=1 WELP_HISTORY=(history --max 10 | string collect) command welp fix $yes)
    or return
    test -n "$fix"; or return
    builtin history append -- $fix 2>/dev/null
    eval $fix
end
"#
            ),
        }
    }

    pub fn config_files(self) -> &'static [&'static str] {
        match self {
            Shell::Bash => &[".bashrc", ".bash_profile"],
            Shell::Zsh => &[".zshrc"],
            Shell::Fish => &[".config/fish/config.fish"],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_takes_yeah() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish] {
            let script = shell.script("fuck");
            assert!(
                script.contains("--yeah") && script.contains("welp fix $yes"),
                "{shell:?}"
            );
        }
    }
}

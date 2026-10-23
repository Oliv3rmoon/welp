use clap::ValueEnum;

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
#[allow(clippy::enum_variant_names)] // PowerShell is its name
pub enum Shell {
    Bash,
    Zsh,
    Fish,
    Tcsh,
    #[value(name = "powershell")]
    PowerShell,
}

impl Shell {
    pub const ALL: [Shell; 5] = [
        Shell::Bash,
        Shell::Zsh,
        Shell::Fish,
        Shell::Tcsh,
        Shell::PowerShell,
    ];
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
            Shell::Tcsh => format!("eval `welp init tcsh{alias}`"),
            Shell::PowerShell => {
                format!("Invoke-Expression (& welp init powershell{alias} | Out-String)")
            }
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
            // tcsh aliases can't hold a function, so this one reads the
            // command before it straight from history and evals the fix.
            Shell::Tcsh => format!(
                "alias {alias} 'setenv WELP_ALIAS {alias}; setenv WELP_HISTORY \"`history -h 2 | head -n 1`\"; set welp_fix=\"`welp fix \\!*`\"; eval \"$welp_fix\"'\n"
            ),
            // `& $welp` calls the program, not this function, when the
            // alias is also called welp.
            Shell::PowerShell => format!(
                r#"function {alias} {{
    $welp = (Get-Command welp -CommandType Application | Select-Object -First 1).Source
    $rest = @($args)
    $yes = @()
    if ($rest.Count -gt 0 -and @('-y', '--yes', '--yeah') -contains $rest[0]) {{
        $yes = @('--yes')
        $rest = @($rest | Select-Object -Skip 1)
    }}
    if ($rest.Count -gt 0) {{ & $welp @rest; return }}
    $env:WELP_ALIAS = '{alias}'
    $env:WELP_SHELL = 'powershell'
    $env:WELP_HISTORY = (Get-History -Count 10 | ForEach-Object {{ $_.CommandLine }}) -join "`n"
    $fix = & $welp fix @yes
    $ok = $LASTEXITCODE -eq 0
    Remove-Item Env:WELP_ALIAS, Env:WELP_SHELL, Env:WELP_HISTORY -ErrorAction SilentlyContinue
    if ($ok -and $fix) {{ Invoke-Expression ($fix -join "`n") }}
}}
"#
            ),
        }
    }

    pub fn config_files(self) -> &'static [&'static str] {
        match self {
            Shell::Bash => &[".bashrc", ".bash_profile"],
            Shell::Zsh => &[".zshrc"],
            Shell::Fish => &[".config/fish/config.fish"],
            Shell::Tcsh => &[".tcshrc", ".cshrc"],
            Shell::PowerShell => &[
                ".config/powershell/Microsoft.PowerShell_profile.ps1",
                "Documents/PowerShell/Microsoft.PowerShell_profile.ps1",
                "Documents/WindowsPowerShell/Microsoft.PowerShell_profile.ps1",
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_shell_takes_yeah() {
        for shell in [Shell::Bash, Shell::Zsh, Shell::Fish, Shell::PowerShell] {
            let script = shell.script("fuck");
            assert!(
                script.contains("--yeah") && script.contains("fix @yes")
                    || script.contains("welp fix $yes"),
                "{shell:?}"
            );
        }
        // tcsh passes the alias's arguments straight through.
        assert!(Shell::Tcsh.script("fuck").contains("welp fix \\!*"));
    }
}

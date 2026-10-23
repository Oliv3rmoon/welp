use std::env;
use std::io::Read;
#[cfg(unix)]
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::{Command as Process, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::config::Config;

/// Runs the command again to read what it printed, unless it is one that
/// should never run twice. Gives up after the configured wait.
pub fn capture_output(script: &str, config: &Config) -> String {
    let program = script
        .split_whitespace()
        .find(|word| *word != "sudo")
        .unwrap_or("");
    if script.trim_start().starts_with("sudo") || config.no_rerun.iter().any(|p| p == program) {
        return String::new();
    }
    let wait = if config.slow_commands.iter().any(|p| p == program) {
        config.wait_slow_command
    } else {
        config.wait_command
    };

    let mut process = shell_process(script);
    process
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    // Its own process group, so a timeout stops everything it started.
    #[cfg(unix)]
    process.process_group(0);
    let Ok(mut child) = process.spawn() else {
        return String::new();
    };

    let mut stdout = child.stdout.take().expect("stdout is piped");
    let reader = thread::spawn(move || {
        let mut buffer = Vec::new();
        let _ = stdout.read_to_end(&mut buffer);
        buffer
    });

    let deadline = Instant::now() + Duration::from_secs(wait);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(10)),
            _ => {
                stop_all(child.id());
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
}

/// A shell that runs `script` the way the user's shell would, with errors
/// sent to the same place as the output. PowerShell when the PowerShell
/// setup asked for it, and on Windows; `sh` everywhere else.
fn shell_process(script: &str) -> Process {
    let powershell =
        cfg!(windows) || env::var("WELP_SHELL").is_ok_and(|shell| shell == "powershell");
    if powershell {
        let exe = if find_in_path("pwsh").is_some() {
            "pwsh"
        } else {
            "powershell"
        };
        let mut process = Process::new(exe);
        process.args([
            "-NoProfile",
            "-NonInteractive",
            "-Command",
            &format!("& {{ {script} }} *>&1"),
        ]);
        process
    } else {
        let mut process = Process::new("sh");
        process.arg("-c").arg(format!("exec 2>&1\n{script}"));
        process
    }
}

/// Stops the process and everything it started.
fn stop_all(pid: u32) {
    // `--` so Linux's kill reads the negative number as a process group.
    #[cfg(unix)]
    let _ = Process::new("kill")
        .args(["-KILL", "--", &format!("-{pid}")])
        .status();
    #[cfg(windows)]
    let _ = Process::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .status();
}

/// Where `program` is on PATH, trying Windows' PATHEXT endings there.
pub fn find_in_path(program: &str) -> Option<PathBuf> {
    let endings: Vec<String> = if cfg!(windows) {
        let pathext = env::var("PATHEXT").unwrap_or_else(|_| ".COM;.EXE;.BAT;.CMD".into());
        std::iter::once(String::new())
            .chain(pathext.split(';').map(str::to_lowercase))
            .collect()
    } else {
        vec![String::new()]
    };
    env::split_paths(&env::var_os("PATH")?).find_map(|dir| {
        endings
            .iter()
            .map(|ending| dir.join(format!("{program}{ending}")))
            .find(|path| path.is_file())
    })
}

/// Runs a read-only listing command (`gradle tasks`, `yarn --help`) and
/// returns what it printed, or `None` if it couldn't run or took over 3 s.
#[allow(dead_code)] // used by the rules being ported
pub fn run_listing(program: &str, args: &[&str]) -> Option<String> {
    find_in_path(program)?;
    let script = std::iter::once(program)
        .chain(args.iter().copied())
        .map(|word| shell_words::quote(word).into_owned())
        .collect::<Vec<_>>()
        .join(" ");
    let config = Config {
        wait_command: 3,
        no_rerun: Vec::new(),
        slow_commands: Vec::new(),
        ..Config::default()
    };
    let output = capture_output(&script, &config);
    (!output.trim().is_empty()).then_some(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_programs_on_path() {
        let shell = if cfg!(windows) { "cmd" } else { "sh" };
        assert!(find_in_path(shell).is_some());
        assert!(find_in_path("welp-no-such-program").is_none());
    }

    #[cfg(unix)]
    #[test]
    fn captures_stderr() {
        let output = capture_output("echo oops >&2", &Config::default());
        assert_eq!(output.trim(), "oops");
    }

    #[test]
    fn never_reruns_listed_commands() {
        assert_eq!(
            capture_output("rm -r /tmp/welp-should-not-run", &Config::default()),
            ""
        );
    }

    #[cfg(unix)]
    #[test]
    fn stops_at_the_wait() {
        let config = Config {
            wait_command: 1,
            ..Config::default()
        };
        let start = Instant::now();
        capture_output("sleep 5", &config);
        assert!(start.elapsed() < Duration::from_secs(3));
    }
}

use std::io::Read;
use std::os::unix::process::CommandExt;
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

    // Its own process group, so a timeout stops everything it started.
    let Ok(mut child) = Process::new("sh")
        .arg("-c")
        .arg(format!("exec 2>&1\n{script}"))
        .process_group(0)
        .env("LC_ALL", "C")
        .env("LANG", "C")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
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
                // `--` so Linux's kill reads the negative number as a process group.
                let _ = Process::new("kill")
                    .args(["-KILL", "--", &format!("-{}", child.id())])
                    .status();
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
        }
    }
    String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned()
}

/// Runs a read-only listing command (`gradle tasks`, `yarn --help`) and
/// returns what it printed, or `None` if it couldn't run or took over 3 s.
#[allow(dead_code)] // used by the rules being ported
pub fn run_listing(program: &str, args: &[&str]) -> Option<String> {
    let on_path = std::env::var_os("PATH")
        .is_some_and(|path| std::env::split_paths(&path).any(|dir| dir.join(program).is_file()));
    if !on_path {
        return None;
    }
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

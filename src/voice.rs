//! Everything welp says, in one place. Lowercase, short, dry, and a shrug
//! when it has nothing.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// The mascot.
pub const SHRUG: &str = r"¯\_(ツ)_/¯";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Voice {
    /// Laid-back, a line now and then.
    #[default]
    Chill,
    /// Teases you a little, every time. Opt in.
    Spicy,
    /// Just the fix.
    Quiet,
}

/// When no rule has a fix.
pub const NO_FIX: &[&str] = &[
    "nothing to fix here. that one's all you.",
    "got nothing. that one might be on the computer.",
    "no idea, honestly. try it again slower?",
    "drawing a blank on that one.",
];

pub const NO_FIX_SPICY: &[&str] = &[
    "not even i can save that one.",
    "i've seen things. that's a new one.",
    "nope. that's all you.",
];

/// When there is no command in history to fix.
pub const NOTHING_TO_FIX: &str = "nothing to fix yet. go break something first.";

/// When you say no to the fix.
pub const ABORTED: &str = "fair enough.";

/// After a fix, for any rule.
pub const AFTER_FIX: &[&str] = &[
    "happens to the best of us.",
    "classic.",
    "muscle memory, huh.",
    "we don't talk about this one.",
    "fixed. nobody saw anything.",
    "your secret's safe.",
];

pub const AFTER_FIX_SPICY: &[&str] = &[
    "keyboard's fine. it's you.",
    "i'll pretend i didn't see that.",
    "we're adding this to the list.",
    "you're lucky i'm here.",
];

/// After a fix from one rule in particular.
fn rule_lines(rule: &str, voice: Voice) -> &'static [&'static str] {
    match (voice, rule) {
        (Voice::Spicy, "sudo") => &["you know you're not root, right?"],
        (Voice::Spicy, "no_command") => &["bold spelling choice."],
        (Voice::Spicy, "git_push") => &["every. single. new. branch."],
        (Voice::Spicy, "cd_parent") => &["one space. you had one job."],
        (_, "sudo") => &["asked nicely this time.", "with great power, etc."],
        (_, "no_command") => &["fingers got ahead of you."],
        (_, "git_push") => &["upstream set. go be productive."],
        (_, "git_push_force") => &["force-with-lease. the polite kind of force."],
        (_, "git_not_command") => &["git knew what you meant. so did i."],
        (_, "git_checkout") => &["new branch, who dis."],
        (_, "git_stash") => &["stashed. it'll be there when you get back."],
        (_, "mkdir_p" | "touch") => &["parents included."],
        (_, "cd_parent" | "cd_correction") => &["close enough."],
        (_, "chmod_x") => &["it's executable now. go wild."],
        (_, "did_you_mean") => &["even the tool knew."],
        (_, "rm_dir") => &["folder's gone. hope you meant it."],
        _ => &[],
    }
}

/// What welp knows about you, for lines that need it.
#[derive(Debug, Default, Clone)]
pub struct Memory {
    /// Fixes so far, this one included.
    pub total: u64,
    /// The typo this fix corrected (`gti`, `git`) and how often it happened.
    pub typo: Option<(String, String, u64)>,
}

const MILESTONES: &[(u64, &str)] = &[
    (1, "first fix. welcome. it only gets worse from here."),
    (10, "10 fixes. we're getting to know each other."),
    (50, "50 fixes. at this point i'm part of your workflow."),
    (100, "100 fixes. that's... a lot of typing."),
    (500, "500 fixes. you might want to look into a keyboard."),
    (1000, "1000 fixes. honestly? impressive."),
];

const REPEATS: &[u64] = &[3, 10, 25, 50, 100];

/// A number that changes from run to run, for picking lines.
pub fn seed() -> u64 {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|time| time.subsec_nanos())
        .unwrap_or(0);
    u64::from(nanos) ^ u64::from(std::process::id())
}

pub fn pick<'a>(lines: &[&'a str], seed: u64) -> &'a str {
    lines[(seed % lines.len() as u64) as usize]
}

pub fn no_fix(voice: Voice, seed: u64) -> String {
    let lines = if voice == Voice::Spicy {
        NO_FIX_SPICY
    } else {
        NO_FIX
    };
    format!("{SHRUG}  {}", pick(lines, seed))
}

/// What to say under a fix, if anything. Milestones and repeat typos always
/// get a line; otherwise chill speaks up about one fix in three and spicy
/// every time.
pub fn after_fix(rule: &str, memory: &Memory, voice: Voice, seed: u64) -> Option<String> {
    if voice == Voice::Quiet {
        return None;
    }
    if let Some((_, line)) = MILESTONES.iter().find(|(count, _)| *count == memory.total) {
        return Some(line.to_string());
    }
    if let Some((wrong, right, count)) = &memory.typo
        && REPEATS.contains(count)
    {
        return Some(match voice {
            Voice::Spicy => format!("{wrong} again? that's {count}. {right} is right there."),
            _ => format!("{wrong} → {right}, {count} times now. we don't have to talk about it."),
        });
    }
    if voice == Voice::Chill && !seed.is_multiple_of(3) {
        return None;
    }
    let specific = rule_lines(rule, voice);
    let general = if voice == Voice::Spicy {
        AFTER_FIX_SPICY
    } else {
        AFTER_FIX
    };
    // Half the time a rule's own line, when it has one.
    let lines = if !specific.is_empty() && (seed / 3).is_multiple_of(2) {
        specific
    } else {
        general
    };
    Some(pick(lines, seed / 7).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn memory(total: u64) -> Memory {
        Memory { total, typo: None }
    }

    #[test]
    fn picks_every_line() {
        let picked: std::collections::BTreeSet<_> =
            (0..20).map(|seed| pick(NO_FIX, seed)).collect();
        assert_eq!(picked.len(), NO_FIX.len());
    }

    #[test]
    fn no_fix_shrugs() {
        assert!(no_fix(Voice::Chill, 0).starts_with(SHRUG));
    }

    #[test]
    fn chill_speaks_now_and_then() {
        let spoken = (0..900)
            .filter(|seed| after_fix("sl_ls", &memory(7), Voice::Chill, *seed).is_some())
            .count();
        assert_eq!(spoken, 300);
    }

    #[test]
    fn spicy_always_speaks_and_quiet_never_does() {
        assert!((0..100).all(|seed| after_fix("sl_ls", &memory(7), Voice::Spicy, seed).is_some()));
        assert!((0..100).all(|seed| after_fix("sl_ls", &memory(1), Voice::Quiet, seed).is_none()));
    }

    #[test]
    fn milestones_and_repeats_always_land() {
        assert_eq!(
            after_fix("sudo", &memory(100), Voice::Chill, 1).unwrap(),
            "100 fixes. that's... a lot of typing."
        );
        let typo = Memory {
            total: 7,
            typo: Some(("gti".into(), "git".into(), 10)),
        };
        assert_eq!(
            after_fix("no_command", &typo, Voice::Chill, 1).unwrap(),
            "gti → git, 10 times now. we don't have to talk about it."
        );
        assert_eq!(
            after_fix("no_command", &typo, Voice::Spicy, 1).unwrap(),
            "gti again? that's 10. git is right there."
        );
    }

    #[test]
    fn rules_get_their_own_lines() {
        let lines: std::collections::BTreeSet<_> = (0..300)
            .filter_map(|seed| after_fix("sudo", &memory(7), Voice::Chill, seed))
            .collect();
        assert!(lines.contains("asked nicely this time."));
        assert!(lines.contains("classic."));
    }

    #[test]
    fn lines_stay_lowercase_and_short() {
        let fixed = [NOTHING_TO_FIX, ABORTED];
        let all = NO_FIX
            .iter()
            .chain(NO_FIX_SPICY)
            .chain(AFTER_FIX)
            .chain(AFTER_FIX_SPICY)
            .chain(fixed.iter());
        for line in all.chain(MILESTONES.iter().map(|(_, line)| line)) {
            assert_eq!(*line, line.to_lowercase(), "{line}");
            assert!(line.chars().count() <= 52, "{line}");
        }
    }
}

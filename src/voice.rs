//! Everything welp says, in one place. Lowercase, short, dry.

use std::time::{SystemTime, UNIX_EPOCH};

/// When no rule has a fix.
pub const NO_FIX: &[&str] = &[
    "nothing to fix here. that one's all you.",
    "got nothing. that one might be on the computer.",
    "no idea, honestly. try it again slower?",
    "drawing a blank on that one.",
];

/// When there is no command in history to fix.
pub const NOTHING_TO_FIX: &str = "nothing to fix yet. go break something first.";

/// When you say no to the fix.
pub const ABORTED: &str = "fair enough.";

/// Now and then, after a fix runs.
pub const AFTER_FIX: &[&str] = &[
    "happens to the best of us.",
    "classic.",
    "muscle memory, huh.",
    "we don't talk about this one.",
    "fixed. nobody saw anything.",
    "your secret's safe.",
];

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

/// About one fix in five gets a line, and none when `quiet` is on.
pub fn after_fix(seed: u64, quiet: bool) -> Option<&'static str> {
    if quiet || !seed.is_multiple_of(5) {
        return None;
    }
    Some(pick(AFTER_FIX, seed / 5))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_every_line() {
        let picked: std::collections::BTreeSet<_> =
            (0..20).map(|seed| pick(NO_FIX, seed)).collect();
        assert_eq!(picked.len(), NO_FIX.len());
    }

    #[test]
    fn after_fix_is_occasional() {
        let spoken = (0..1000)
            .filter(|seed| after_fix(*seed, false).is_some())
            .count();
        assert_eq!(spoken, 200);
    }

    #[test]
    fn quiet_says_nothing() {
        assert!((0..1000).all(|seed| after_fix(seed, true).is_none()));
    }

    #[test]
    fn lines_stay_lowercase_and_short() {
        for line in NO_FIX
            .iter()
            .chain(AFTER_FIX)
            .chain([&NOTHING_TO_FIX, &ABORTED])
        {
            assert_eq!(*line, line.to_lowercase(), "{line}");
            assert!(line.len() <= 50, "{line}");
        }
    }
}

//! More shell, file and system rules ported from thefuck.

use super::Rule;

/// Rules in this file that are off unless the user enables them.
pub const DISABLED: &[&str] = &[];

pub fn rules() -> Vec<Rule> {
    vec![]
}

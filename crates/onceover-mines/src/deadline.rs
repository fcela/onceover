//! A cooperative deadline for long pass loops (issue #117,
//! stage 2).
//!
//! The profile-level wall-clock deadline used to be consulted
//! only between passes, so one expensive pass could overrun it
//! by its whole runtime: the dependence screen over a wide table
//! or the discontinuities pass could run for tens of seconds
//! past the caller's budget. Passes with long internal loops
//! now poll a `Deadline` at documented work quanta and stop
//! cooperatively, returning the work completed so far; the
//! pass's coverage entry then discloses exactly how much of
//! its search space was examined, so a partial negative is
//! distinguishable from an exhaustive one.
//!
//! Semantics: with no deadline set (the default) every check
//! is one branch on `None` and results are unchanged —
//! determinism is the default contract. With a deadline set,
//! *where* a pass stops is timing-dependent (the deadline is
//! already documented as an opt-in source of
//! nondeterminism), but *what* was examined is never
//! arbitrary: stopping happens only at quantum boundaries in
//! the pass's deterministic enumeration order, so the
//! examined set is always a prefix of that order, and the
//! coverage entry states its size in the pass's own units.

use std::time::Instant;

/// A point in time after which cooperative pass loops stop.
/// `Copy` and cheap to poll: one comparison per work quantum.
#[derive(Clone, Copy, Debug, Default)]
pub struct Deadline {
    at: Option<Instant>,
}

impl Deadline {
    /// No deadline: checks never fire.
    pub fn none() -> Self {
        Self { at: None }
    }

    /// A deadline at an absolute instant (the pipeline builds
    /// it as profile start + the configured duration, so every
    /// pass shares the profile's single budget).
    pub fn at(instant: Instant) -> Self {
        Self { at: Some(instant) }
    }

    pub fn is_set(&self) -> bool {
        self.at.is_some()
    }

    /// True when a deadline is set and has been reached.
    pub fn exceeded(&self) -> bool {
        self.at.is_some_and(|t| Instant::now() >= t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn unset_deadline_never_fires() {
        assert!(!Deadline::none().is_set());
        assert!(!Deadline::none().exceeded());
        assert!(!Deadline::default().exceeded());
    }

    #[test]
    fn past_deadline_fires_future_does_not() {
        let past = Deadline::at(Instant::now() - Duration::from_secs(1));
        assert!(past.is_set());
        assert!(past.exceeded());
        let future = Deadline::at(Instant::now() + Duration::from_secs(60));
        assert!(future.is_set());
        assert!(!future.exceeded());
    }
}

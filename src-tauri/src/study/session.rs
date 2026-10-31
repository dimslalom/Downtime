//! Where the player is. Viewing, revealing, grading and being interrupted are
//! tracked apart, because only a grade may ever touch learning history.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::flashcards::Card;

/// A window left open while the player wanders off is not study time.
const MAX_SEGMENT_MS: i64 = 120_000;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Attempt {
    /// Also the id of the review a grade creates, which makes grading idempotent.
    pub id: String,
    pub card_key: String,
    pub prompt_viewed_ms: Option<i64>,
    pub revealed_ms: Option<i64>,
    pub flipped: bool,
    pub active_ms: u64,
    /// Set only while the overlay is open and focused.
    pub resumed_ms: Option<i64>,
    pub interruptions: u32,
}

impl Attempt {
    pub fn resume(&mut self, now: i64) {
        self.prompt_viewed_ms.get_or_insert(now);
        self.resumed_ms.get_or_insert(now);
    }

    /// Stops the clock and returns the time added. Time spent paused never counts.
    pub fn pause(&mut self, now: i64) -> u64 {
        let Some(resumed) = self.resumed_ms.take() else {
            return 0;
        };
        let added = (now - resumed).clamp(0, MAX_SEGMENT_MS) as u64;
        self.active_ms += added;
        added
    }

    /// After a crash the open segment's end is unknown, so it is dropped.
    pub fn discard_running_segment(&mut self) {
        if self.resumed_ms.take().is_some() {
            self.interruptions += 1;
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Session {
    pub deck_uid: String,
    pub deck_name: String,
    pub day: i64,
    /// A snapshot: the deck file changing does not touch a session in progress.
    pub cards: Vec<Card>,
    pub passed: u32,
    pub failed: u32,
    /// The player asked to keep going past what was scheduled.
    pub extra: bool,
    pub current: Option<Attempt>,
    pub last_card: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct Sessions {
    pub active: Option<String>,
    pub by_deck: BTreeMap<String, Session>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_time_between_resume_and_pause_counts() {
        let mut attempt = Attempt::default();
        attempt.resume(1_000);
        assert_eq!(attempt.prompt_viewed_ms, Some(1_000));
        assert_eq!(attempt.pause(4_000), 3_000);

        attempt.resume(100_000);
        assert_eq!(attempt.prompt_viewed_ms, Some(1_000));
        assert_eq!(attempt.pause(102_000), 2_000);
        assert_eq!(attempt.active_ms, 5_000);
    }

    #[test]
    fn pausing_twice_adds_nothing() {
        let mut attempt = Attempt::default();
        attempt.resume(0);
        attempt.pause(1_000);
        assert_eq!(attempt.pause(9_000), 0);
    }

    #[test]
    fn a_window_left_open_is_capped() {
        let mut attempt = Attempt::default();
        attempt.resume(0);
        assert_eq!(attempt.pause(3_600_000), MAX_SEGMENT_MS as u64);
    }

    #[test]
    fn a_crash_drops_the_open_segment_and_counts_an_interruption() {
        let mut attempt = Attempt::default();
        attempt.resume(0);
        attempt.discard_running_segment();
        assert_eq!(attempt.active_ms, 0);
        assert_eq!(attempt.interruptions, 1);
        assert_eq!(attempt.resumed_ms, None);
    }
}

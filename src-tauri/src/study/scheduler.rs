//! Decides what to show next. Reads memory state and never writes it.

use super::memory::{LearningState, MemoryModel, Moment, Phase};

/// A card can come round at most this often in a study day, so one stubborn card
/// cannot take over a session. It returns the next day.
pub const MAX_DAILY_EXPOSURES: u32 = 3;

pub struct QueueCard<'a> {
    pub key: &'a str,
    pub state: &'a LearningState,
    pub flagged: bool,
    /// Position in the deck, which is the order new cards are introduced.
    pub position: usize,
}

fn elapsed_days(state: &LearningState, at: Moment) -> f32 {
    state.last_review_ms.map_or(0.0, |last| (at.ms - last) as f32 / 86_400_000.0)
}

fn retrievability(model: &MemoryModel, state: &LearningState, at: Moment) -> f32 {
    state
        .memory
        .map_or(0.0, |memory| model.retrievability(memory, elapsed_days(state, at)))
}

fn in_flight(card: &QueueCard, at: Moment) -> bool {
    matches!(card.state.phase, Phase::Learning | Phase::Relearning) && card.state.is_due(at)
}

fn available(card: &QueueCard, at: Moment) -> bool {
    card.state.exposures_on(at.study_day()) < MAX_DAILY_EXPOSURES
}

fn weakest_first<'a>(
    mut cards: Vec<&'a QueueCard<'a>>,
    model: &MemoryModel,
    at: Moment,
) -> Vec<&'a QueueCard<'a>> {
    cards.sort_by(|a, b| {
        retrievability(model, a.state, at)
            .total_cmp(&retrievability(model, b.state, at))
            .then(a.position.cmp(&b.position))
    });
    cards
}

/// Standard FSRS order: cards mid-step, then due reviews, then new cards up to
/// the daily limit. Flagged cards wait behind everything else.
pub fn ordinary_queue<'a>(
    cards: &'a [QueueCard<'a>],
    model: &MemoryModel,
    at: Moment,
    new_allowance: u32,
) -> Vec<&'a str> {
    let open: Vec<&QueueCard> = cards.iter().filter(|card| available(card, at)).collect();
    let (flagged, open): (Vec<_>, Vec<_>) = open.into_iter().partition(|card| card.flagged);

    let steps = open.iter().copied().filter(|card| in_flight(card, at)).collect();
    let due = open
        .iter()
        .copied()
        .filter(|card| card.state.phase == Phase::Review && card.state.is_due(at))
        .collect();
    let new = open
        .iter()
        .copied()
        .filter(|card| card.state.phase == Phase::New)
        .take(new_allowance as usize);
    let flagged_due = flagged.iter().copied().filter(|card| card.state.is_due(at)).collect();

    weakest_first(steps, model, at)
        .into_iter()
        .chain(weakest_first(due, model, at))
        .chain(new)
        .chain(weakest_first(flagged_due, model, at))
        .map(|card| card.key)
        .collect()
}

/// Deadline order: cards mid-step, then today's plan. A card the plan did not
/// pick is not shown even if FSRS calls it due, and one it picked is shown even
/// if FSRS would wait. That is what lets the plan change when reviews happen.
pub fn deadline_queue<'a>(
    cards: &'a [QueueCard<'a>],
    plan_today: &[String],
    model: &MemoryModel,
    at: Moment,
) -> Vec<&'a str> {
    let open: Vec<&QueueCard> = cards.iter().filter(|card| available(card, at)).collect();
    let steps = open
        .iter()
        .copied()
        .filter(|card| !card.flagged && in_flight(card, at))
        .collect();
    let day = at.study_day();
    let planned = plan_today.iter().filter_map(|key| {
        open.iter().copied().find(|card| {
            card.key == key
                && !card.flagged
                && !in_flight(card, at)
                && card.state.last_review_day != Some(day)
        })
    });
    let flagged_due = open
        .iter()
        .copied()
        .filter(|card| card.flagged && card.state.is_due(at))
        .collect();

    weakest_first(steps, model, at)
        .into_iter()
        .chain(planned)
        .chain(weakest_first(flagged_due, model, at))
        .map(|card| card.key)
        .collect()
}

/// For a player who asks for more once nothing is scheduled: the weakest cards
/// not yet seen today, then any remaining new cards, then flagged ones. These are
/// real early reviews, which FSRS credits according to their actual timing.
pub fn extra_queue<'a>(
    cards: &'a [QueueCard<'a>],
    model: &MemoryModel,
    at: Moment,
) -> Vec<&'a str> {
    let day = at.study_day();
    let open: Vec<&QueueCard> = cards.iter().filter(|card| available(card, at)).collect();
    let (flagged, open): (Vec<_>, Vec<_>) = open.into_iter().partition(|card| card.flagged);
    let seen = open
        .iter()
        .copied()
        .filter(|card| {
            card.state.phase != Phase::New
                && card.state.last_review_day.is_none_or(|last| last < day)
        })
        .collect();
    let new = open.iter().copied().filter(|card| card.state.phase == Phase::New);

    weakest_first(seen, model, at)
        .into_iter()
        .chain(new)
        .chain(weakest_first(flagged, model, at))
        .map(|card| card.key)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::study::memory::{Rating, DAY_MS};

    fn at(day: i64) -> Moment {
        Moment {
            ms: (20_000 + day) * DAY_MS + 12 * 3_600_000,
            utc_offset_min: 0,
        }
    }

    fn reviewed(day: i64) -> LearningState {
        MemoryModel::default()
            .apply(&LearningState::default(), Rating::Good, at(day), 0.9)
            .unwrap()
    }

    fn card<'a>(key: &'a str, state: &'a LearningState, position: usize) -> QueueCard<'a> {
        QueueCard {
            key,
            state,
            flagged: false,
            position,
        }
    }

    #[test]
    fn orders_due_reviews_before_new_cards_and_honours_the_new_limit() {
        let model = MemoryModel::default();
        let (older, newer, new) = (reviewed(0), reviewed(1), LearningState::default());
        let cards = [
            card("new-a", &new, 0),
            card("new-b", &new, 1),
            card("newer", &newer, 2),
            card("older", &older, 3),
        ];
        let queue = ordinary_queue(&cards, &model, at(5), 1);
        assert_eq!(queue, ["older", "newer", "new-a"]);
    }

    #[test]
    fn cards_that_are_not_due_are_left_alone() {
        let model = MemoryModel::default();
        let state = reviewed(0);
        let cards = [card("a", &state, 0)];
        assert!(ordinary_queue(&cards, &model, at(1), 5).is_empty());
    }

    #[test]
    fn flagged_cards_wait_behind_everything_and_stay_reachable() {
        let model = MemoryModel::default();
        let (flagged_state, other, new) = (reviewed(0), reviewed(1), LearningState::default());
        let mut flagged = card("flagged", &flagged_state, 0);
        flagged.flagged = true;
        let cards = [flagged, card("other", &other, 1), card("new", &new, 2)];
        assert_eq!(ordinary_queue(&cards, &model, at(5), 5), ["other", "new", "flagged"]);
    }

    #[test]
    fn repeated_failures_cannot_monopolise_a_day() {
        let model = MemoryModel::default();
        let mut state = LearningState::default();
        let mut now = at(0);
        for _ in 0..MAX_DAILY_EXPOSURES {
            state = model.apply(&state, Rating::Again, now, 0.9).unwrap();
            now.ms += DAY_MS / 24;
        }
        let cards = [card("stuck", &state, 0)];
        assert!(ordinary_queue(&cards, &model, now, 5).is_empty());
        let tomorrow = Moment { ms: now.ms + DAY_MS, ..now };
        assert_eq!(ordinary_queue(&cards, &model, tomorrow, 5), ["stuck"]);
    }

    #[test]
    fn the_plan_can_bring_a_card_forward_and_hold_a_due_card_back() {
        let model = MemoryModel::default();
        let (early, held) = (reviewed(4), reviewed(0));
        let cards = [card("early", &early, 0), card("held", &held, 1)];
        let plan = vec!["early".to_owned()];
        assert_eq!(deadline_queue(&cards, &plan, &model, at(5)), ["early"]);
    }

    #[test]
    fn extra_practice_skips_what_was_seen_today() {
        let model = MemoryModel::default();
        let (today, older, new) = (reviewed(5), reviewed(4), LearningState::default());
        let cards = [card("today", &today, 0), card("older", &older, 1), card("new", &new, 2)];
        assert_eq!(extra_queue(&cards, &model, at(5)), ["older", "new"]);
    }
}

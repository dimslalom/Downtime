//! Per-card memory, modelled by the official `fsrs` crate. Nothing here knows
//! about deadlines or importance: those shape the plan, never the model.

use fsrs::{
    compute_parameters, current_retrievability, ComputeParametersInput, FSRSItem, FSRSReview,
    MemoryState, DEFAULT_PARAMETERS, FSRS,
};
use serde::{Deserialize, Serialize};

pub const MODEL_VERSION: &str = "fsrs-rs 6.6.2 (FSRS-6)";
pub const DAY_MS: i64 = 86_400_000;
const MINUTE_MS: i64 = 60_000;
/// A late-night session belongs to the day it started, as in Anki.
const DAY_ROLLOVER_MS: i64 = 4 * 3_600_000;
const LEARNING_STEP_MS: i64 = 10 * MINUTE_MS;
const RELEARNING_STEP_MS: i64 = 10 * MINUTE_MS;
const MAX_INTERVAL_DAYS: f32 = 36_500.0;
/// Below this many long-term training items the fit is reported as provisional.
const SOLID_FIT_ITEMS: usize = 400;

/// A point in time with the UTC offset in force, so that day boundaries replay
/// the same way after travel or a daylight-saving change.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Moment {
    pub ms: i64,
    pub utc_offset_min: i32,
}

impl Moment {
    pub fn study_day(self) -> i64 {
        (self.ms + i64::from(self.utc_offset_min) * MINUTE_MS - DAY_ROLLOVER_MS).div_euclid(DAY_MS)
    }

    pub fn day_start_ms(self, day: i64) -> i64 {
        day * DAY_MS + DAY_ROLLOVER_MS - i64::from(self.utc_offset_min) * MINUTE_MS
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Rating {
    Again = 1,
    Hard = 2,
    Good = 3,
    Easy = 4,
}

impl Rating {
    pub fn passed(self) -> bool {
        self != Rating::Again
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Phase {
    #[default]
    New,
    Learning,
    Review,
    Relearning,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub struct Memory {
    pub stability: f32,
    pub difficulty: f32,
}

impl From<MemoryState> for Memory {
    fn from(state: MemoryState) -> Self {
        Self {
            stability: state.stability,
            difficulty: state.difficulty,
        }
    }
}

impl From<Memory> for MemoryState {
    fn from(memory: Memory) -> Self {
        Self {
            stability: memory.stability,
            difficulty: memory.difficulty,
        }
    }
}

/// Derived entirely from the raw review log; `replay` reconstructs it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct LearningState {
    pub phase: Phase,
    pub memory: Option<Memory>,
    pub last_review_ms: Option<i64>,
    pub last_review_day: Option<i64>,
    /// Learning steps are timed; graduated cards are due on a study day.
    pub due_ms: Option<i64>,
    pub due_day: Option<i64>,
    pub reps: u32,
    pub lapses: u32,
    pub exposure_day: i64,
    pub exposures: u32,
}

impl LearningState {
    pub fn is_due(&self, at: Moment) -> bool {
        match self.phase {
            Phase::New => false,
            Phase::Learning | Phase::Relearning => self.due_ms.is_some_and(|due| due <= at.ms),
            Phase::Review => self.due_day.is_some_and(|due| due <= at.study_day()),
        }
    }

    pub fn exposures_on(&self, day: i64) -> u32 {
        if self.exposure_day == day {
            self.exposures
        } else {
            0
        }
    }
}

#[derive(Clone, Debug)]
pub struct MemoryModel {
    fsrs: FSRS,
    pub parameters: Vec<f32>,
}

impl Default for MemoryModel {
    fn default() -> Self {
        Self::new(&DEFAULT_PARAMETERS).expect("default parameters are valid")
    }
}

impl MemoryModel {
    pub fn new(parameters: &[f32]) -> Result<Self, String> {
        let fsrs = FSRS::new(parameters).map_err(|error| error.to_string())?;
        let parameters = if parameters.is_empty() {
            DEFAULT_PARAMETERS.to_vec()
        } else {
            parameters.to_vec()
        };
        Ok(Self { fsrs, parameters })
    }

    fn decay(&self) -> f32 {
        self.parameters.get(20).copied().unwrap_or(fsrs::FSRS6_DEFAULT_DECAY)
    }

    pub fn retrievability(&self, memory: Memory, days: f32) -> f32 {
        current_retrievability(memory.into(), days.max(0.0), self.decay())
    }

    /// What FSRS predicts for each rating after `days_elapsed` days. Zero days
    /// is the library's same-day case, which only applies a short-term update.
    pub fn transitions(
        &self,
        memory: Option<Memory>,
        desired_retention: f32,
        days_elapsed: u32,
    ) -> Result<[(Memory, f32); 4], String> {
        let next = self
            .fsrs
            .next_states(memory.map(Into::into), desired_retention, days_elapsed)
            .map_err(|error| error.to_string())?;
        Ok([next.again, next.hard, next.good, next.easy]
            .map(|item| (item.memory.into(), item.interval)))
    }

    fn graduation_day(&self, interval: f32, day: i64) -> i64 {
        day + interval.round().clamp(1.0, MAX_INTERVAL_DAYS) as i64
    }

    /// The only way a card's memory changes: one explicit grade. Same-day and
    /// early repeats go through FSRS with their real elapsed days, so they count
    /// for less than a delayed recall.
    pub fn apply(
        &self,
        state: &LearningState,
        rating: Rating,
        at: Moment,
        desired_retention: f32,
    ) -> Result<LearningState, String> {
        let day = at.study_day();
        let elapsed = state
            .last_review_day
            .map_or(0, |last| (day - last).max(0) as u32);
        let transitions = self.transitions(state.memory, desired_retention, elapsed)?;
        let (memory, interval) = transitions[rating as usize - 1];

        let mut next = state.clone();
        next.memory = Some(memory);
        next.last_review_ms = Some(at.ms);
        next.last_review_day = Some(day);
        next.reps += 1;
        next.exposures = state.exposures_on(day) + 1;
        next.exposure_day = day;

        let (phase, step_ms) = match (state.phase, rating) {
            (Phase::New | Phase::Learning, Rating::Again | Rating::Hard) => {
                (Phase::Learning, Some(LEARNING_STEP_MS))
            }
            (Phase::Review, Rating::Again) => {
                next.lapses += 1;
                (Phase::Relearning, Some(RELEARNING_STEP_MS))
            }
            (Phase::Relearning, Rating::Again | Rating::Hard) => {
                (Phase::Relearning, Some(RELEARNING_STEP_MS))
            }
            _ => (Phase::Review, None),
        };
        next.phase = phase;
        next.due_ms = step_ms.map(|step| at.ms + step);
        next.due_day = step_ms.is_none().then(|| self.graduation_day(interval, day));
        Ok(next)
    }

    /// The due day for a graduated card under a retention target.
    pub fn due_day_for(&self, state: &LearningState, desired_retention: f32) -> Option<i64> {
        let memory = state.memory?;
        let interval = self
            .fsrs
            .next_interval(Some(memory.stability), desired_retention, 3);
        Some(self.graduation_day(interval, state.last_review_day?))
    }
}

pub fn default_parameters() -> Vec<f32> {
    DEFAULT_PARAMETERS.to_vec()
}

/// Turns one card's reviews into the optimizer's items: one per delayed review,
/// each with the history before it. Same-day repeats stay in the history but are
/// never a prediction target, as the library requires.
pub fn training_items(days_and_ratings: &[(i64, Rating)]) -> Vec<FSRSItem> {
    let mut reviews = Vec::with_capacity(days_and_ratings.len());
    let mut items = Vec::new();
    let mut previous = None;
    for &(day, rating) in days_and_ratings {
        let delta_t = previous.map_or(0, |last: i64| (day - last).max(0) as u32);
        reviews.push(FSRSReview {
            rating: rating as u32,
            delta_t,
        });
        previous = Some(day);
        if delta_t > 0 {
            items.push(FSRSItem {
                reviews: reviews.clone(),
            });
        }
    }
    items
}

#[derive(Debug, PartialEq)]
pub enum FitOutcome {
    Fitted {
        parameters: Vec<f32>,
        limited_data: bool,
    },
    /// The library returns its defaults when it has too little to learn from.
    NotEnoughData,
}

/// Blocking and potentially slow; never call this from the review path. A panic
/// inside the optimizer counts as a failure so the old parameters stay in use.
pub fn fit_parameters(items: Vec<FSRSItem>) -> Result<FitOutcome, String> {
    let item_count = items.len();
    if item_count == 0 {
        return Ok(FitOutcome::NotEnoughData);
    }
    let fitted = std::panic::catch_unwind(|| {
        compute_parameters(ComputeParametersInput {
            train_set: items,
            ..Default::default()
        })
    })
    .map_err(|_| "The optimizer stopped unexpectedly.".to_owned())?
    .map_err(|error| error.to_string())?;

    if fitted == DEFAULT_PARAMETERS {
        return Ok(FitOutcome::NotEnoughData);
    }
    MemoryModel::new(&fitted)?;
    Ok(FitOutcome::Fitted {
        parameters: fitted,
        limited_data: item_count < SOLID_FIT_ITEMS,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY0: i64 = 20_000 * DAY_MS;

    fn at(day: i64, hour: i64) -> Moment {
        Moment {
            ms: DAY0 + day * DAY_MS + hour * 3_600_000,
            utc_offset_min: 0,
        }
    }

    fn grade(model: &MemoryModel, state: &LearningState, rating: Rating, when: Moment) -> LearningState {
        model.apply(state, rating, when, 0.9).unwrap()
    }

    fn graduated(model: &MemoryModel) -> LearningState {
        grade(model, &LearningState::default(), Rating::Good, at(0, 12))
    }

    #[test]
    fn study_days_roll_over_at_four_in_the_morning_local_time() {
        let late = Moment { ms: at(1, 3).ms, utc_offset_min: 0 };
        let early = Moment { ms: at(1, 5).ms, utc_offset_min: 0 };
        assert_eq!(early.study_day() - late.study_day(), 1);

        let brisbane = Moment { ms: at(1, 20).ms, utc_offset_min: 600 };
        assert_eq!(brisbane.study_day() - at(1, 20).study_day(), 1);
        assert_eq!(
            Moment { ms: at(1, 12).day_start_ms(at(1, 12).study_day()), utc_offset_min: 0 }.study_day(),
            at(1, 12).study_day()
        );
    }

    #[test]
    fn first_ratings_match_the_library_defaults() {
        let model = MemoryModel::default();
        let good = graduated(&model);
        assert_eq!(good.phase, Phase::Review);
        let memory = good.memory.unwrap();
        assert!((memory.stability - 2.3065).abs() < 1e-4);
        assert_eq!(good.due_day, Some(at(0, 12).study_day() + 2));
    }

    #[test]
    fn again_means_failed_recall_and_counts_a_lapse_only_on_a_graduated_card() {
        let model = MemoryModel::default();
        let new = grade(&model, &LearningState::default(), Rating::Again, at(0, 12));
        assert_eq!(new.phase, Phase::Learning);
        assert_eq!(new.lapses, 0);
        assert_eq!(new.due_ms, Some(at(0, 12).ms + LEARNING_STEP_MS));

        let review = graduated(&model);
        let lapsed = grade(&model, &review, Rating::Again, at(3, 12));
        assert_eq!(lapsed.phase, Phase::Relearning);
        assert_eq!(lapsed.lapses, 1);
        assert!(lapsed.memory.unwrap().stability < review.memory.unwrap().stability);
    }

    #[test]
    fn hard_is_a_success_that_still_grows_stability_and_never_lapses() {
        let model = MemoryModel::default();
        let review = graduated(&model);
        let hard = grade(&model, &review, Rating::Hard, at(3, 12));
        assert_eq!(hard.phase, Phase::Review);
        assert_eq!(hard.lapses, 0);
        assert!(hard.memory.unwrap().stability > review.memory.unwrap().stability);
    }

    #[test]
    fn stability_orders_again_hard_good_easy() {
        let model = MemoryModel::default();
        let review = graduated(&model);
        let stabilities = [Rating::Again, Rating::Hard, Rating::Good, Rating::Easy]
            .map(|rating| grade(&model, &review, rating, at(3, 12)).memory.unwrap().stability);
        assert!(stabilities.windows(2).all(|pair| pair[0] < pair[1]), "{stabilities:?}");
    }

    #[test]
    fn same_day_repeats_gain_far_less_than_a_delayed_recall() {
        let model = MemoryModel::default();
        let review = graduated(&model);
        let same_day = grade(&model, &review, Rating::Good, at(0, 13));
        let delayed = grade(&model, &review, Rating::Good, at(2, 12));
        let base = review.memory.unwrap().stability;
        let rapid_gain = same_day.memory.unwrap().stability - base;
        let delayed_gain = delayed.memory.unwrap().stability - base;
        assert!(rapid_gain < delayed_gain / 2.0, "{rapid_gain} vs {delayed_gain}");
    }

    #[test]
    fn retrievability_falls_as_time_passes() {
        let model = MemoryModel::default();
        let memory = graduated(&model).memory.unwrap();
        let (now, later, much_later) = (
            model.retrievability(memory, 0.0),
            model.retrievability(memory, memory.stability),
            model.retrievability(memory, 30.0),
        );
        assert!((now - 1.0).abs() < 1e-6);
        assert!((later - 0.9).abs() < 1e-3);
        assert!(much_later < later);
    }

    #[test]
    fn desired_retention_changes_the_interval_but_not_the_memory() {
        let model = MemoryModel::default();
        let eager = model.apply(&LearningState::default(), Rating::Easy, at(0, 12), 0.95).unwrap();
        let relaxed = model.apply(&LearningState::default(), Rating::Easy, at(0, 12), 0.8).unwrap();
        assert_eq!(eager.memory, relaxed.memory);
        assert!(eager.due_day.unwrap() < relaxed.due_day.unwrap());
    }

    #[test]
    fn training_items_only_target_delayed_reviews() {
        let items = training_items(&[
            (0, Rating::Again),
            (0, Rating::Good),
            (3, Rating::Good),
            (3, Rating::Again),
            (9, Rating::Good),
        ]);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].reviews.len(), 3);
        assert_eq!(items[0].reviews[0].delta_t, 0);
        assert_eq!(items[1].reviews.last().unwrap().delta_t, 6);
    }

    #[test]
    fn sparse_history_reports_not_enough_data_instead_of_a_fake_fit() {
        let items = training_items(&[(0, Rating::Good), (2, Rating::Good)]);
        assert_eq!(fit_parameters(items), Ok(FitOutcome::NotEnoughData));
        assert_eq!(fit_parameters(Vec::new()), Ok(FitOutcome::NotEnoughData));
    }

    #[test]
    fn invalid_parameters_are_rejected() {
        assert!(MemoryModel::new(&[1.0, 2.0]).is_err());
        assert!(MemoryModel::new(&[]).is_ok());
    }
}

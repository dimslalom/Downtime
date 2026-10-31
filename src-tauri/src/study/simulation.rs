//! Offline comparison of deadline planning against ordinary FSRS.
//!
//! Each run replays one simulated learner on a scenario under one policy. Both
//! policies face the same learner, starting state, downtime and luck: every random
//! draw comes from a stream keyed by what it decides, not by the order of events.
//! The learner's true memory can differ from the planner's model, which sees only
//! the ratings a real player would give. Only `plan_seed` touches the planner, and
//! only `eval_seed` touches the learner.
//!
//! This compares models against models. It cannot show that either policy works
//! better for real people.

use serde::Serialize;

use super::memory::{LearningState, MemoryModel, Moment, Phase, Rating, DAY_MS};
use super::planner::{
    plan, BudgetGroup, CostModel, PlanCard, PlanDeck, PlanOutput, PlanRequest, RatingModel,
};
use super::scheduler::{deadline_queue, ordinary_queue, QueueCard};
use super::store::SchedulingSettings;

const BASE_DAY: i64 = 20_000;
const EVENING_MS: i64 = 18 * 3_600_000;
const EXAM_HOUR_FROM_ROLLOVER: f64 = 5.0 / 24.0;
const REPLAN_EVERY_REVIEWS: u32 = 20;
const MAX_SLOTS_PER_DAY: u32 = 60;
const PLANNING_RELIABILITY: f32 = 0.7;

/// SplitMix64, kept here so results do not depend on a dependency's version.
#[derive(Clone, Copy)]
struct Rng(u64);

impl Rng {
    fn keyed(seed: u64, stream: u64, index: u64) -> Self {
        let mut rng = Self(seed ^ stream.wrapping_mul(0x9e37_79b9_7f4a_7c15));
        rng.next();
        rng.0 ^= index.wrapping_mul(0xbf58_476d_1ce4_e5b9);
        rng.next();
        rng
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut mixed = self.0;
        mixed = (mixed ^ (mixed >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        mixed = (mixed ^ (mixed >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        mixed ^ (mixed >> 31)
    }

    fn unit(&mut self) -> f32 {
        (self.next() >> 40) as f32 / (1u64 << 24) as f32
    }

    fn normal(&mut self) -> f32 {
        let (a, b) = (self.unit().max(1e-6), self.unit());
        (-2.0 * a.ln()).sqrt() * (std::f32::consts::TAU * b).cos()
    }
}

const STREAM_MEMORY: u64 = 1;
const STREAM_DAY: u64 = 2;
const STREAM_REVIEW: u64 = 3;
const STREAM_HISTORY: u64 = 4;
const STREAM_IMPORTANCE: u64 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Policy {
    /// Standard FSRS with the app's default limit on new cards per day.
    Ordinary,
    /// Standard FSRS with no such limit, a stronger baseline when time is short.
    OrdinaryUncapped,
    Deadline,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub enum Learner {
    /// Forgets exactly as the planner's default parameters assume.
    Matched,
    /// Memories form and grow less than the planner assumes.
    FastForgetter,
    /// Memories form and grow more than the planner assumes.
    SlowForgetter,
    /// A differently shaped forgetting curve from the planner's.
    SteeperCurve,
}

impl Learner {
    pub const ALL: [Learner; 4] = [
        Learner::Matched,
        Learner::FastForgetter,
        Learner::SlowForgetter,
        Learner::SteeperCurve,
    ];

    fn model(self) -> MemoryModel {
        let mut parameters = MemoryModel::default().parameters;
        match self {
            Learner::Matched => {}
            Learner::FastForgetter => {
                parameters[..4].iter_mut().for_each(|value| *value *= 0.6);
                parameters[8] -= 0.4;
            }
            Learner::SlowForgetter => {
                parameters[..4].iter_mut().for_each(|value| *value *= 1.6);
                parameters[8] += 0.3;
            }
            Learner::SteeperCurve => parameters[20] = 0.45,
        }
        MemoryModel::new(&parameters).expect("learner parameters are valid")
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct Scenario {
    pub name: &'static str,
    pub cards: usize,
    pub days: i64,
    pub daily_minutes: f32,
    /// Share of cards already seen once or twice before day 0.
    pub seen_share: f32,
    pub missed_day_rate: f32,
    pub interruption_rate: f32,
    pub unequal_importance: bool,
}

impl Scenario {
    const fn base(name: &'static str, cards: usize, days: i64, daily_minutes: f32) -> Self {
        Self {
            name,
            cards,
            days,
            daily_minutes,
            seen_share: 0.0,
            missed_day_rate: 0.1,
            interruption_rate: 0.1,
            unequal_importance: false,
        }
    }
}

/// Fixed before any results were seen, and not to be tuned to favour a policy.
pub fn scenarios() -> Vec<Scenario> {
    vec![
        Scenario::base("short deadline", 150, 5, 15.0),
        Scenario::base("long deadline", 400, 45, 15.0),
        Scenario { seen_share: 0.6, ..Scenario::base("sparse history", 200, 14, 10.0) },
        Scenario { missed_day_rate: 0.4, ..Scenario::base("missed days", 200, 21, 10.0) },
        Scenario { unequal_importance: true, ..Scenario::base("unequal importance", 200, 14, 10.0) },
        Scenario { unequal_importance: true, ..Scenario::base("long, unequal importance", 400, 45, 15.0) },
        Scenario::base("mostly unseen, too little time", 600, 10, 10.0),
        Scenario { interruption_rate: 0.35, ..Scenario::base("interrupted sessions", 150, 14, 10.0) },
        Scenario::base("ample time", 80, 21, 30.0),
    ]
}

#[derive(Clone, Debug, Serialize)]
pub struct Metrics {
    pub weighted_recall: f32,
    pub recall: f32,
    pub coverage: f32,
    pub study_minutes: f32,
    pub over_budget_days: u32,
    pub reviews: u32,
    /// Share of all reviews that went to the most-reviewed tenth of cards.
    pub top_decile_share: f32,
    pub max_reviews_per_card: u32,
}

struct SimCard {
    key: String,
    importance: u8,
    believed: LearningState,
    truth: LearningState,
    /// Per-card variation in how quickly it is forgotten.
    durability: f32,
    reviews: u32,
}

fn moment(day: i64, offset_ms: i64) -> Moment {
    Moment {
        ms: (BASE_DAY + day) * DAY_MS + EVENING_MS + offset_ms,
        utc_offset_min: 0,
    }
}

fn days_between(from_ms: i64, to_ms: i64) -> f32 {
    ((to_ms - from_ms) as f32 / DAY_MS as f32).max(0.0)
}

fn true_recall(card: &SimCard, model: &MemoryModel, at_ms: i64) -> f32 {
    match (card.truth.memory, card.truth.last_review_ms) {
        (Some(memory), Some(last)) => {
            model.retrievability(memory, days_between(last, at_ms) / card.durability)
        }
        _ => 0.0,
    }
}

/// What the player presses. Rating style depends on how the recall felt, not on
/// any scheduling state.
fn rate(recall: f32, is_new: bool, rng: &mut Rng) -> Rating {
    if is_new {
        return match rng.unit() {
            roll if roll < 0.3 => Rating::Again,
            roll if roll < 0.45 => Rating::Hard,
            roll if roll < 0.92 => Rating::Good,
            _ => Rating::Easy,
        };
    }
    if rng.unit() >= recall {
        return Rating::Again;
    }
    let roll = rng.unit();
    if recall < 0.7 && roll < 0.35 {
        Rating::Hard
    } else if recall > 0.95 && roll < 0.3 {
        Rating::Easy
    } else {
        Rating::Good
    }
}

fn seconds_for(rating: Rating, is_new: bool, rng: &mut Rng) -> f32 {
    let base = match (is_new, rating) {
        (true, _) => 16.0,
        (false, Rating::Again) => 16.0,
        _ => 10.0,
    };
    base * (0.3 * rng.normal()).exp()
}

fn starting_cards(scenario: &Scenario, believed: &MemoryModel, truth: &MemoryModel, eval_seed: u64) -> Vec<SimCard> {
    (0..scenario.cards)
        .map(|index| {
            let mut memory_rng = Rng::keyed(eval_seed, STREAM_MEMORY, index as u64);
            let mut importance_rng = Rng::keyed(eval_seed, STREAM_IMPORTANCE, index as u64);
            let importance = if scenario.unequal_importance {
                1 + (importance_rng.unit() * 5.0) as u8
            } else {
                3
            };
            let mut card = SimCard {
                key: format!("c{index}"),
                importance: importance.min(5),
                believed: LearningState::default(),
                truth: LearningState::default(),
                durability: (0.3 * memory_rng.normal()).exp(),
                reviews: 0,
            };

            let mut history = Rng::keyed(eval_seed, STREAM_HISTORY, index as u64);
            if history.unit() < scenario.seen_share {
                let looks = 1 + (history.unit() * 2.0) as i64;
                for look in 0..looks {
                    let day = -1 - (looks - look) * (1 + (history.unit() * 3.0) as i64);
                    let at = moment(day, 0);
                    let is_new = card.truth.memory.is_none();
                    let recall = true_recall(&card, truth, at.ms);
                    let rating = rate(recall, is_new, &mut history);
                    card.truth = truth.apply(&card.truth, rating, at, 0.9).expect("valid state");
                    card.believed = believed.apply(&card.believed, rating, at, 0.9).expect("valid state");
                    if rating == Rating::Again || card.believed.phase != Phase::Review {
                        let later = moment(day, 600_000);
                        card.truth = truth.apply(&card.truth, Rating::Good, later, 0.9).expect("valid state");
                        card.believed =
                            believed.apply(&card.believed, Rating::Good, later, 0.9).expect("valid state");
                    }
                }
            }
            card
        })
        .collect()
}

struct DayPlanner {
    today: Vec<String>,
    since_plan: u32,
}

fn make_plan(
    scenario: &Scenario,
    cards: &[SimCard],
    believed: &MemoryModel,
    day: i64,
    spent_s: f32,
    plan_seed: u64,
) -> PlanOutput {
    let budget_s = scenario.daily_minutes * 60.0;
    let today = BASE_DAY + day;
    let request = PlanRequest {
        today,
        decks: vec![PlanDeck {
            uid: "deck".into(),
            exam_offset_days: (scenario.days - day) as f64 + EXAM_HOUR_FROM_ROLLOVER,
            group: 0,
            max_new_today: None,
            cards: cards
                .iter()
                .enumerate()
                .map(|(position, card)| PlanCard {
                    key: card.key.clone(),
                    importance: card.importance,
                    memory: card.believed.memory,
                    last_day: card.believed.last_review_day,
                    frozen: false,
                    position,
                })
                .collect(),
        }],
        groups: vec![BudgetGroup {
            today_s: (budget_s - spent_s).max(0.0),
            daily_s: budget_s * PLANNING_RELIABILITY,
        }],
        costs: CostModel::default(),
        ratings: RatingModel::default(),
        seed: plan_seed,
    };
    plan(believed, &request)
}

fn pick(
    policy: Policy,
    cards: &[SimCard],
    believed: &MemoryModel,
    planner: &DayPlanner,
    at: Moment,
    new_left: u32,
    last: Option<usize>,
) -> Option<usize> {
    let queue_cards: Vec<QueueCard> = cards
        .iter()
        .enumerate()
        .map(|(position, card)| QueueCard {
            key: &card.key,
            state: &card.believed,
            flagged: false,
            position,
        })
        .collect();
    let queue = match policy {
        Policy::Ordinary => ordinary_queue(&queue_cards, believed, at, new_left),
        Policy::OrdinaryUncapped => ordinary_queue(&queue_cards, believed, at, u32::MAX),
        Policy::Deadline => deadline_queue(&queue_cards, &planner.today, believed, at),
    };
    let index_of = |key: &str| cards.iter().position(|card| card.key == key);
    queue
        .iter()
        .filter_map(|key| index_of(key))
        .find(|&index| Some(index) != last)
        .or_else(|| queue.first().and_then(|key| index_of(key)))
}

pub fn run(scenario: &Scenario, learner: Learner, policy: Policy, plan_seed: u64, eval_seed: u64) -> Metrics {
    let believed = MemoryModel::default();
    let truth = learner.model();
    let retention = SchedulingSettings::default().desired_retention;
    let daily_new = SchedulingSettings::default().new_per_day;
    let budget_s = scenario.daily_minutes * 60.0;
    let mut cards = starting_cards(scenario, &believed, &truth, eval_seed);
    let (mut total_s, mut over_budget_days) = (0.0_f32, 0_u32);

    for day in 0..scenario.days {
        let mut rng = Rng::keyed(eval_seed, STREAM_DAY, day as u64);
        if rng.unit() < scenario.missed_day_rate {
            continue;
        }
        // Downtime is irregular: some days give only part of the estimate.
        let available = budget_s * (0.5 + 0.5 * rng.unit());
        let mut spent = 0.0_f32;
        let mut clock_ms = 0_i64;
        let mut new_today = 0_u32;
        let mut last = None;
        let mut pending: Option<(usize, Rating, f32)> = None;
        let mut planner = DayPlanner { today: Vec::new(), since_plan: REPLAN_EVERY_REVIEWS };

        for _ in 0..MAX_SLOTS_PER_DAY {
            if spent >= available {
                break;
            }
            let mut left = (15.0 + 30.0 * rng.unit()).min(available - spent);
            if rng.unit() < scenario.interruption_rate {
                left *= rng.unit();
            }
            while left > 0.0 {
                if policy == Policy::Deadline && planner.since_plan >= REPLAN_EVERY_REVIEWS {
                    let output = make_plan(scenario, &cards, &believed, day, spent, plan_seed);
                    planner.today.clone_from(&output.decks[0].today);
                    planner.since_plan = 0;
                }
                let at = moment(day, clock_ms);
                if pending.is_none() {
                    let allowance = daily_new.saturating_sub(new_today);
                    let Some(index) = pick(policy, &cards, &believed, &planner, at, allowance, last) else {
                        break;
                    };
                    let card = &cards[index];
                    let is_new = card.truth.memory.is_none();
                    let mut review_rng = Rng::keyed(
                        eval_seed ^ (index as u64).wrapping_mul(0x1000_0001),
                        STREAM_REVIEW,
                        u64::from(card.reviews) + (day as u64) << 12,
                    );
                    let rating = rate(true_recall(card, &truth, at.ms), is_new, &mut review_rng);
                    pending = Some((index, rating, seconds_for(rating, is_new, &mut review_rng)));
                }
                let (index, rating, remaining) = pending.take().expect("set above");
                if remaining > left {
                    // The slot ended mid-card: nothing is graded and it resumes later.
                    pending = Some((index, rating, remaining - left));
                    spent += left;
                    break;
                }
                spent += remaining;
                left -= remaining;
                let card = &mut cards[index];
                new_today += u32::from(card.believed.phase == Phase::New);
                card.truth = truth.apply(&card.truth, rating, at, retention).expect("valid state");
                card.believed = believed.apply(&card.believed, rating, at, retention).expect("valid state");
                card.reviews += 1;
                last = Some(index);
                planner.since_plan += 1;
                clock_ms += (remaining * 1000.0) as i64;
            }
            // The gap until the next death, queue or spectate.
            clock_ms += 180_000 + (rng.unit() * 1_320_000.0) as i64;
        }
        total_s += spent;
        over_budget_days += u32::from(spent > budget_s);
    }

    evaluate(scenario, &cards, &truth, total_s, over_budget_days)
}

fn evaluate(scenario: &Scenario, cards: &[SimCard], truth: &MemoryModel, total_s: f32, over_budget_days: u32) -> Metrics {
    let exam_ms = (BASE_DAY + scenario.days) * DAY_MS + 9 * 3_600_000;
    let count = cards.len() as f32;
    let (mut weighted, mut weights, mut plain) = (0.0, 0.0, 0.0);
    for card in cards {
        let recall = true_recall(card, truth, exam_ms);
        let weight = f32::from(card.importance);
        weighted += weight * recall;
        weights += weight;
        plain += recall;
    }
    let mut reviews: Vec<u32> = cards.iter().map(|card| card.reviews).collect();
    reviews.sort_unstable_by(|a, b| b.cmp(a));
    let total: u32 = reviews.iter().sum();
    let top: u32 = reviews.iter().take(cards.len().div_ceil(10)).sum();
    Metrics {
        weighted_recall: weighted / weights,
        recall: plain / count,
        coverage: cards.iter().filter(|card| card.truth.memory.is_some()).count() as f32 / count,
        study_minutes: total_s / 60.0,
        over_budget_days,
        reviews: total,
        top_decile_share: if total == 0 { 0.0 } else { top as f32 / total as f32 },
        max_reviews_per_card: reviews.first().copied().unwrap_or(0),
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    fn small() -> Scenario {
        Scenario { unequal_importance: true, ..Scenario::base("test", 60, 8, 6.0) }
    }

    #[test]
    fn identical_seeds_reproduce_identical_results() {
        let scenario = small();
        let first = run(&scenario, Learner::FastForgetter, Policy::Deadline, 1, 2);
        let second = run(&scenario, Learner::FastForgetter, Policy::Deadline, 1, 2);
        assert_eq!(first.weighted_recall, second.weighted_recall);
        assert_eq!(first.reviews, second.reviews);
    }

    #[test]
    fn the_evaluation_seed_changes_the_learner_not_just_the_plan() {
        let scenario = small();
        let first = run(&scenario, Learner::Matched, Policy::Ordinary, 1, 2);
        let second = run(&scenario, Learner::Matched, Policy::Ordinary, 1, 3);
        assert_ne!(first.weighted_recall, second.weighted_recall);
    }

    #[test]
    fn the_plan_seed_cannot_affect_the_ordinary_policy() {
        let scenario = small();
        let first = run(&scenario, Learner::Matched, Policy::Ordinary, 1, 2);
        let second = run(&scenario, Learner::Matched, Policy::Ordinary, 99, 2);
        assert_eq!(first.weighted_recall, second.weighted_recall);
    }

    #[test]
    fn both_policies_stay_within_the_budget() {
        let scenario = small();
        for policy in [Policy::Ordinary, Policy::Deadline] {
            let metrics = run(&scenario, Learner::Matched, policy, 1, 2);
            assert!(metrics.study_minutes <= 6.0 * 8.0 + 1.0, "{metrics:?}");
            assert_eq!(metrics.over_budget_days, 0, "{metrics:?}");
            assert!(metrics.reviews > 0, "{metrics:?}");
        }
    }

    #[test]
    fn learners_differ_from_the_planners_assumptions() {
        let scenario = small();
        let matched = run(&scenario, Learner::Matched, Policy::Ordinary, 1, 2);
        let fast = run(&scenario, Learner::FastForgetter, Policy::Ordinary, 1, 2);
        let slow = run(&scenario, Learner::SlowForgetter, Policy::Ordinary, 1, 2);
        assert!(fast.recall < matched.recall, "{fast:?} {matched:?}");
        assert!(slow.recall > fast.recall, "{slow:?} {fast:?}");
    }

    #[test]
    fn no_single_card_takes_over_a_run() {
        let metrics = run(&small(), Learner::FastForgetter, Policy::Deadline, 1, 2);
        assert!(metrics.top_decile_share < 0.5, "{metrics:?}");
        assert!(metrics.max_reviews_per_card <= 3 * 8, "{metrics:?}");
    }
}

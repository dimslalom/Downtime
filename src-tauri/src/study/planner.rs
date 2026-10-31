//! Deadline planning over FSRS. See docs/deadline-mode.md for the full account.
//!
//! Objective: maximise predicted importance-weighted recall at the exam, subject
//! to the estimated study time left. Importance is a utility weight here and
//! nowhere else: it never touches memory parameters or memory state.
//!
//! Approach: Lagrangian relaxation. Study seconds on each future day get a price.
//! Given prices, each card is an independent small decision problem, solved by
//! backward induction over FSRS's own transitions on a stability/difficulty grid:
//! at each decision day, either stop or pick the next review day, weighing the
//! Again/Hard/Good/Easy outcomes, their time cost, and everything they lead to.
//! Prices rise on days the cards collectively overspend (dual ascent), which
//! shares a budget across decks and stops every card piling up before the exam.
//! Today's list is the cards whose review is worth most per second over waiting.
//!
//! This is an approximation with no optimality guarantee: state is interpolated,
//! far-off days are coarsened, outcomes use average rating shares, and the rolling
//! replan is what corrects drift. It does not change any state; it only reads.

use std::collections::HashMap;

use serde::Serialize;

use super::memory::{Memory, MemoryModel};

const STABILITY_NODES: usize = 26;
const STABILITY_MIN: f32 = 0.05;
const STABILITY_MAX: f32 = 3000.0;
const DIFFICULTY_NODES: [f32; 4] = [1.0, 4.0, 7.0, 10.0];
const NODES: usize = STABILITY_NODES * DIFFICULTY_NODES.len();
const ITERATIONS: usize = 48;
const PRICE_STEP: f32 = 0.02;
/// Utility a second of the player's time must earn, even with budget to spare.
/// At neutral importance a 12-second review has to add about 0.6 points of exam
/// recall, so spare time is not spent re-reading what is already secure.
const MIN_PRICE: f32 = 0.0015;
/// Chance per day of waiting that a planned review still happens. Gaming time
/// is irregular, so what is only planned is worth less than what is done, which
/// stops the planner deferring everything towards the exam.
const DEFERRAL_SURVIVAL: f32 = 0.97;
const FULL_DETAIL_DAYS: i32 = 6;
const EXAM_DETAIL_DAYS: i32 = 5;
/// Reviews land mid-day on average, so a review on the last day still ages a bit.
const REVIEW_TIME_OF_DAY: f64 = 0.5;
const MIN_TIME_LEFT: f64 = 0.02;
/// `next_states` needs a retention to size intervals; the planner ignores those.
const UNUSED_RETENTION: f32 = 0.9;

#[derive(Clone, Copy, Debug)]
pub struct CostModel {
    pub new_s: f32,
    pub pass_s: f32,
    pub fail_s: f32,
}

impl Default for CostModel {
    /// Round figures for the 15-second card the deck format aims for.
    fn default() -> Self {
        Self {
            new_s: 18.0,
            pass_s: 12.0,
            fail_s: 18.0,
        }
    }
}

/// How ratings tend to split: `review` among successes, `new` over first answers.
#[derive(Clone, Copy, Debug)]
pub struct RatingModel {
    pub review: [f32; 3],
    pub new: [f32; 4],
}

impl Default for RatingModel {
    fn default() -> Self {
        Self {
            review: [0.15, 0.75, 0.10],
            new: [0.25, 0.15, 0.50, 0.10],
        }
    }
}

#[derive(Clone, Debug)]
pub struct PlanCard {
    pub key: String,
    pub importance: u8,
    pub memory: Option<Memory>,
    pub last_day: Option<i64>,
    /// Flagged: left out of rapid review but still counted in the estimates.
    pub frozen: bool,
    pub position: usize,
}

#[derive(Clone, Debug)]
pub struct PlanDeck {
    pub uid: String,
    /// Days from the start of today's study day to the exam.
    pub exam_offset_days: f64,
    pub group: usize,
    pub max_new_today: Option<u32>,
    pub cards: Vec<PlanCard>,
}

/// Decks in one group compete for its seconds. `today_s` is what is left today;
/// `daily_s` is the realistic expectation for later days.
#[derive(Clone, Copy, Debug)]
pub struct BudgetGroup {
    pub today_s: f32,
    pub daily_s: f32,
}

#[derive(Clone, Debug)]
pub struct PlanRequest {
    pub today: i64,
    pub decks: Vec<PlanDeck>,
    pub groups: Vec<BudgetGroup>,
    pub costs: CostModel,
    pub ratings: RatingModel,
    /// Breaks ties among otherwise equal cards. Zero keeps deck order.
    pub seed: u64,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckEstimate {
    pub exam_recall_weighted: f32,
    pub exam_recall: f32,
    pub no_study_recall_weighted: f32,
    pub unseen_now: usize,
    pub frozen: usize,
    pub expected_introduced: f32,
    pub expected_reviews: f32,
    pub expected_minutes: f32,
    pub today_minutes: f32,
}

#[derive(Clone, Debug, Default)]
pub struct DeckPlan {
    pub uid: String,
    pub today: Vec<String>,
    /// Worth doing if the player has more time than the budget assumed.
    pub stretch: Vec<String>,
    pub estimate: DeckEstimate,
}

#[derive(Clone, Debug, Default)]
pub struct PlanOutput {
    pub day: i64,
    pub decks: Vec<DeckPlan>,
}

#[derive(Clone, Copy)]
struct Interp {
    node: u16,
    next_s: u16,
    next_d: u16,
    fs: f32,
    fd: f32,
}

#[derive(Clone, Copy)]
struct Outcome {
    probability: f32,
    cost: f32,
    at: Interp,
}

fn stability_at(index: usize) -> f32 {
    let step = (STABILITY_MAX / STABILITY_MIN).ln() / (STABILITY_NODES - 1) as f32;
    STABILITY_MIN * (step * index as f32).exp()
}

fn locate(memory: Memory) -> Interp {
    let step = (STABILITY_MAX / STABILITY_MIN).ln() / (STABILITY_NODES - 1) as f32;
    let position = ((memory.stability.max(STABILITY_MIN) / STABILITY_MIN).ln() / step)
        .clamp(0.0, (STABILITY_NODES - 1) as f32 - 1e-3);
    let s = position.floor() as usize;

    let d_position = ((memory.difficulty - DIFFICULTY_NODES[0]) / 3.0)
        .clamp(0.0, (DIFFICULTY_NODES.len() - 1) as f32 - 1e-3);
    let d = d_position.floor() as usize;

    Interp {
        node: (s * DIFFICULTY_NODES.len() + d) as u16,
        next_s: DIFFICULTY_NODES.len() as u16,
        next_d: 1,
        fs: position - s as f32,
        fd: d_position - d as f32,
    }
}

#[inline]
fn sample(table: &[f32], at: Interp) -> f32 {
    let node = at.node as usize;
    let (next_s, next_d) = (at.next_s as usize, at.next_d as usize);
    let low = table[node] * (1.0 - at.fd) + table[node + next_d] * at.fd;
    let high = table[node + next_s] * (1.0 - at.fd) + table[node + next_s + next_d] * at.fd;
    low * (1.0 - at.fs) + high * at.fs
}

#[inline]
fn deposit(table: &mut [f32], at: Interp, mass: f32) {
    let node = at.node as usize;
    let (next_s, next_d) = (at.next_s as usize, at.next_d as usize);
    table[node] += mass * (1.0 - at.fs) * (1.0 - at.fd);
    table[node + next_d] += mass * (1.0 - at.fs) * at.fd;
    table[node + next_s] += mass * at.fs * (1.0 - at.fd);
    table[node + next_s + next_d] += mass * at.fs * at.fd;
}

/// Every day near today and near an exam, thinning out in between, so a distant
/// exam costs no more to plan than a close one.
fn decision_days(last_days: &[i32]) -> Vec<i32> {
    let last = last_days.iter().copied().max().unwrap_or(0);
    let mut days: Vec<i32> = (0..=FULL_DETAIL_DAYS.min(last)).collect();
    let (mut day, mut gap) = (FULL_DETAIL_DAYS as f32, 2.0_f32);
    while (day + gap) < last as f32 {
        day += gap;
        days.push(day as i32);
        gap *= 1.4;
    }
    for &deck_last in last_days {
        days.extend((0..EXAM_DETAIL_DAYS).map(|back| deck_last - back).filter(|&day| day >= 0));
    }
    days.sort_unstable();
    days.dedup();
    days
}

struct Outcomes<'a> {
    model: &'a MemoryModel,
    costs: CostModel,
    ratings: RatingModel,
}

impl Outcomes<'_> {
    /// After Again the relearning step follows the same day, so the card carries
    /// on from one more short-term update, at the cost of one more look.
    fn after_relearning(&self, memory: Memory) -> Option<Memory> {
        let transitions = self.model.transitions(Some(memory), UNUSED_RETENTION, 0).ok()?;
        Some(transitions[2].0)
    }

    fn review(&self, memory: Memory, gap: u32) -> Option<[Outcome; 4]> {
        let transitions = self.model.transitions(Some(memory), UNUSED_RETENTION, gap).ok()?;
        let recall = self.model.retrievability(memory, gap as f32);
        let [hard, good, easy] = self.ratings.review;
        let pass = self.costs.pass_s;
        Some([
            Outcome {
                probability: 1.0 - recall,
                cost: self.costs.fail_s + pass,
                at: locate(self.after_relearning(transitions[0].0)?),
            },
            Outcome { probability: recall * hard, cost: pass, at: locate(transitions[1].0) },
            Outcome { probability: recall * good, cost: pass, at: locate(transitions[2].0) },
            Outcome { probability: recall * easy, cost: pass, at: locate(transitions[3].0) },
        ])
    }

    fn introduce(&self) -> Option<[Outcome; 4]> {
        let transitions = self.model.transitions(None, UNUSED_RETENTION, 0).ok()?;
        let [again, hard, good, easy] = self.ratings.new;
        let (first, step) = (self.costs.new_s, self.costs.pass_s);
        Some([
            Outcome {
                probability: again,
                cost: first + step,
                at: locate(self.after_relearning(transitions[0].0)?),
            },
            Outcome {
                probability: hard,
                cost: first + step,
                at: locate(self.after_relearning(transitions[1].0)?),
            },
            Outcome { probability: good, cost: first, at: locate(transitions[2].0) },
            Outcome { probability: easy, cost: first, at: locate(transitions[3].0) },
        ])
    }
}

fn expected_cost(outcomes: &[Outcome; 4]) -> f32 {
    outcomes.iter().map(|outcome| outcome.probability * outcome.cost).sum()
}

/// Cards that plan identically: same deck, importance, rounded memory and gap.
struct Entry {
    deck: usize,
    tier: usize,
    members: Vec<usize>,
    unseen: bool,
    /// Recall at the exam if never reviewed again.
    stop_recall: f32,
    options: Vec<ReviewOption>,
}

struct ReviewOption {
    epoch: usize,
    outcomes: [Outcome; 4],
    cost: f32,
    /// Chance this review actually happens, given it is `epoch` days off.
    survival: f32,
}

struct DeckTables {
    epochs: usize,
    /// Recall at the exam from each node if that review is the last, epochs * NODES.
    stop: Vec<f32>,
    /// By tier: value and recall after a review, epochs * NODES.
    value: Vec<Vec<f32>>,
    recall: Vec<Vec<f32>>,
    next: Vec<Vec<i16>>,
}

struct Shared {
    days: Vec<i32>,
    gap_index: Vec<Vec<usize>>,
    /// gaps * NODES of each node's outcomes.
    transitions: Vec<[Outcome; 4]>,
    gap_cost: Vec<f32>,
}

impl Shared {
    fn build(days: &[i32], outcomes: &Outcomes) -> Self {
        let mut gaps: HashMap<i32, usize> = HashMap::new();
        let mut ordered: Vec<i32> = Vec::new();
        let gap_index: Vec<Vec<usize>> = days
            .iter()
            .map(|&from| {
                days.iter()
                    .map(|&to| {
                        let gap = (to - from).max(1);
                        *gaps.entry(gap).or_insert_with(|| {
                            ordered.push(gap);
                            ordered.len() - 1
                        })
                    })
                    .collect()
            })
            .collect();

        let identity = Outcome {
            probability: 0.0,
            cost: 0.0,
            at: locate(Memory { stability: 1.0, difficulty: 5.0 }),
        };
        let mut transitions = Vec::with_capacity(ordered.len() * NODES);
        let mut gap_cost = Vec::with_capacity(ordered.len() * NODES);
        for &gap in &ordered {
            for s in 0..STABILITY_NODES {
                for &difficulty in &DIFFICULTY_NODES {
                    let memory = Memory { stability: stability_at(s), difficulty };
                    let row = outcomes.review(memory, gap as u32).unwrap_or([identity; 4]);
                    gap_cost.push(expected_cost(&row));
                    transitions.push(row);
                }
            }
        }
        Self {
            days: days.to_vec(),
            gap_index,
            transitions,
            gap_cost,
        }
    }

    fn row(&self, from: usize, to: usize, node: usize) -> (&[Outcome; 4], f32) {
        let index = self.gap_index[from][to] * NODES + node;
        (&self.transitions[index], self.gap_cost[index])
    }
}

fn weight(tier: usize) -> f32 {
    (tier + 1) as f32
}

fn node_slice(table: &[f32], epoch: usize) -> &[f32] {
    &table[epoch * NODES..(epoch + 1) * NODES]
}

/// Backward induction: from each grid state after a review, the best next step.
fn solve_tier(shared: &Shared, tables: &mut DeckTables, tier: usize, prices: &[f32]) {
    let w = weight(tier);
    let epochs = tables.epochs;
    let stop = &tables.stop;
    let (value, recall, next) = (
        &mut tables.value[tier],
        &mut tables.recall[tier],
        &mut tables.next[tier],
    );

    for epoch in (0..epochs).rev() {
        for node in 0..NODES {
            let index = epoch * NODES + node;
            let stop_value = w * stop[index];
            let mut best = stop_value;
            let mut best_recall = stop[index];
            let mut choice = -1_i16;

            for later in epoch + 1..epochs {
                let (row, cost) = shared.row(epoch, later, node);
                let (later_value, later_recall) = (node_slice(value, later), node_slice(recall, later));
                let survival = DEFERRAL_SURVIVAL.powi(shared.days[later] - shared.days[epoch]);
                let mut gain = -prices[later] * cost;
                let mut expected_recall = 0.0;
                for outcome in row {
                    gain += outcome.probability * sample(later_value, outcome.at);
                    expected_recall += outcome.probability * sample(later_recall, outcome.at);
                }
                let at_risk = stop_value + survival * (gain - stop_value);
                if at_risk > best {
                    best = at_risk;
                    best_recall = stop[index] + survival * (expected_recall - stop[index]);
                    choice = later as i16;
                }
            }
            value[index] = best;
            recall[index] = best_recall;
            next[index] = choice;
        }
    }
}

fn option_gain(option: &ReviewOption, price: f32, value: &[f32]) -> f32 {
    let continuation: f32 = option
        .outcomes
        .iter()
        .map(|outcome| outcome.probability * sample(node_slice(value, option.epoch), outcome.at))
        .sum();
    continuation - price * option.cost
}

/// What waiting for this review is worth, allowing that it may never happen.
fn gain_at_risk(option: &ReviewOption, price: f32, value: &[f32], stop_value: f32) -> f32 {
    stop_value + option.survival * (option_gain(option, price, value) - stop_value)
}

fn best_option(entry: &Entry, tables: &DeckTables, prices: &[f32]) -> Option<usize> {
    let value = &tables.value[entry.tier];
    let stop_value = weight(entry.tier) * entry.stop_recall;
    let mut best = stop_value;
    let mut choice = None;
    for (index, option) in entry.options.iter().enumerate() {
        let gain = gain_at_risk(option, prices[option.epoch], value, stop_value);
        if gain > best {
            best = gain;
            choice = Some(index);
        }
    }
    choice
}

#[derive(Clone, Copy, Default)]
struct Totals {
    weighted_recall: f32,
    recall: f32,
    introduced: f32,
    reviews: f32,
    seconds: f32,
}

impl Totals {
    fn add(&mut self, other: Totals) {
        self.weighted_recall += other.weighted_recall;
        self.recall += other.recall;
        self.introduced += other.introduced;
        self.reviews += other.reviews;
        self.seconds += other.seconds;
    }
}

/// Follows the policy forward from today's cards, in expectation, adding up
/// the seconds it wants on each day and the recall it ends with.
fn flow_tier(
    shared: &Shared,
    tables: &DeckTables,
    tier: usize,
    entries: &[&Entry],
    prices: &[f32],
    loads: &mut [f32],
) -> Totals {
    let w = weight(tier);
    let mut totals = Totals::default();
    let mut mass = vec![0.0_f32; tables.epochs * NODES];
    let recall_table = &tables.recall[tier];

    for entry in entries {
        let count = entry.members.len() as f32;
        let Some(index) = best_option(entry, tables, prices) else {
            totals.recall += count * entry.stop_recall;
            totals.weighted_recall += count * w * entry.stop_recall;
            continue;
        };
        let option = &entry.options[index];
        loads[option.epoch] += count * option.cost;
        totals.seconds += count * option.cost;
        totals.reviews += count;
        if entry.unseen {
            totals.introduced += count;
        }
        let mut recall = 0.0;
        for outcome in &option.outcomes {
            let share = count * outcome.probability;
            let slice = option.epoch * NODES..(option.epoch + 1) * NODES;
            deposit(&mut mass[slice], outcome.at, share);
            recall += share * sample(node_slice(recall_table, option.epoch), outcome.at);
        }
        let stopped = count * entry.stop_recall;
        let recall = stopped + option.survival * (recall - stopped);
        totals.recall += recall;
        totals.weighted_recall += w * recall;
    }

    for epoch in 0..tables.epochs {
        for node in 0..NODES {
            let here = mass[epoch * NODES + node];
            if here <= 1e-6 {
                continue;
            }
            let Ok(later) = usize::try_from(tables.next[tier][epoch * NODES + node]) else {
                continue;
            };
            let (row, cost) = shared.row(epoch, later, node);
            loads[later] += here * cost;
            totals.seconds += here * cost;
            totals.reviews += here;
            for outcome in row {
                let slice = later * NODES..(later + 1) * NODES;
                deposit(&mut mass[slice], outcome.at, here * outcome.probability);
            }
        }
    }
    totals
}

fn last_decision_day(exam_offset_days: f64) -> i32 {
    (exam_offset_days - REVIEW_TIME_OF_DAY).floor().max(0.0) as i32
}

fn time_left(exam_offset_days: f64, day: i32) -> f64 {
    (exam_offset_days - (f64::from(day) + REVIEW_TIME_OF_DAY)).max(MIN_TIME_LEFT)
}

fn build_tables(deck: &PlanDeck, days: &[i32], model: &MemoryModel) -> DeckTables {
    let last = last_decision_day(deck.exam_offset_days);
    let epochs = days.iter().filter(|&&day| day <= last).count().max(1);
    let mut stop = Vec::with_capacity(epochs * NODES);
    for &day in &days[..epochs] {
        let left = time_left(deck.exam_offset_days, day) as f32;
        for s in 0..STABILITY_NODES {
            for &difficulty in &DIFFICULTY_NODES {
                let memory = Memory { stability: stability_at(s), difficulty };
                stop.push(model.retrievability(memory, left));
            }
        }
    }
    let table = vec![0.0_f32; epochs * NODES];
    DeckTables {
        epochs,
        stop,
        value: vec![table.clone(); 5],
        recall: vec![table; 5],
        next: vec![vec![-1_i16; epochs * NODES]; 5],
    }
}

fn build_entries(
    request: &PlanRequest,
    tables: &[DeckTables],
    days: &[i32],
    outcomes: &Outcomes,
) -> Vec<Entry> {
    let mut lookup: HashMap<(usize, usize, i32, i32, i64, bool), usize> = HashMap::new();
    let mut entries: Vec<Entry> = Vec::new();

    for (deck_index, deck) in request.decks.iter().enumerate() {
        let epochs = tables[deck_index].epochs;
        for (card_index, card) in deck.cards.iter().enumerate() {
            let tier = usize::from(card.importance.clamp(1, 5)) - 1;
            let gap = card.last_day.map_or(0, |last| (request.today - last).max(0));
            let key = (
                deck_index,
                tier,
                card.memory.map_or(i32::MIN, |memory| (memory.stability.max(1e-3).ln() * 8.0).round() as i32),
                card.memory.map_or(0, |memory| (memory.difficulty * 2.0).round() as i32),
                gap,
                card.frozen,
            );
            if let Some(&existing) = lookup.get(&key) {
                entries[existing].members.push(card_index);
                continue;
            }

            let stop_recall = card.memory.map_or(0.0, |memory| {
                let days = gap as f64 + deck.exam_offset_days - REVIEW_TIME_OF_DAY;
                outcomes.model.retrievability(memory, days.max(MIN_TIME_LEFT) as f32)
            });
            let mut options = Vec::new();
            if !card.frozen {
                for (epoch, &day) in days[..epochs].iter().enumerate() {
                    let row = match card.memory {
                        None => outcomes.introduce(),
                        Some(_) if gap + i64::from(day) == 0 => None,
                        Some(memory) => outcomes.review(memory, (gap + i64::from(day)) as u32),
                    };
                    if let Some(row) = row {
                        options.push(ReviewOption {
                            epoch,
                            cost: expected_cost(&row),
                            outcomes: row,
                            survival: DEFERRAL_SURVIVAL.powi(day),
                        });
                    }
                }
            }
            lookup.insert(key, entries.len());
            entries.push(Entry {
                deck: deck_index,
                tier,
                members: vec![card_index],
                unseen: card.memory.is_none(),
                stop_recall,
                options,
            });
        }
    }
    entries
}

fn capacities(group: BudgetGroup, days: &[i32]) -> Vec<f32> {
    days.iter()
        .enumerate()
        .map(|(index, &day)| {
            let span = days.get(index + 1).map_or(1, |next| next - day) as f32;
            if index == 0 {
                group.today_s + group.daily_s * (span - 1.0)
            } else {
                group.daily_s * span
            }
        })
        .collect()
}

struct Candidate {
    index: f32,
    tie: u64,
    deck: usize,
    card: usize,
    cost: f32,
    unseen: bool,
}

/// Ranks today's possible reviews by what each is worth per second over the
/// best alternative of waiting or stopping, then fills each group's time left.
fn rank_today(
    request: &PlanRequest,
    entries: &[Entry],
    tables: &[DeckTables],
    prices: &[Vec<f32>],
) -> Vec<DeckPlan> {
    let mut candidates = Vec::new();
    for entry in entries {
        let Some(today) = entry.options.iter().find(|option| option.epoch == 0) else {
            continue;
        };
        let deck = &request.decks[entry.deck];
        let table = &tables[entry.deck];
        let deck_prices = &prices[deck.group];
        let value = &table.value[entry.tier];

        let stop_value = weight(entry.tier) * entry.stop_recall;
        let review_now = option_gain(today, 0.0, value);
        let wait = entry
            .options
            .iter()
            .filter(|option| option.epoch != 0)
            .map(|option| gain_at_risk(option, deck_prices[option.epoch], value, stop_value))
            .fold(stop_value, f32::max);
        let index = (review_now - wait) / today.cost.max(1.0);
        if index <= MIN_PRICE {
            continue;
        }
        for &card in &entry.members {
            let tie = if request.seed == 0 {
                deck.cards[card].position as u64
            } else {
                u64::from_str_radix(
                    &crate::flashcards::stable_hash(&format!("{}{}", request.seed, deck.cards[card].key)),
                    16,
                )
                .unwrap_or(0)
            };
            candidates.push(Candidate {
                index,
                tie,
                deck: entry.deck,
                card,
                cost: today.cost,
                unseen: entry.unseen,
            });
        }
    }
    candidates.sort_by(|a, b| b.index.total_cmp(&a.index).then(a.tie.cmp(&b.tie)));

    let mut plans: Vec<DeckPlan> = request
        .decks
        .iter()
        .map(|deck| DeckPlan { uid: deck.uid.clone(), ..DeckPlan::default() })
        .collect();
    let mut left: Vec<f32> = request.groups.iter().map(|group| group.today_s).collect();
    let mut new_taken = vec![0_u32; request.decks.len()];
    for candidate in candidates {
        let deck = &request.decks[candidate.deck];
        let key = deck.cards[candidate.card].key.clone();
        let over_new_limit = candidate.unseen
            && deck.max_new_today.is_some_and(|limit| new_taken[candidate.deck] >= limit);
        let plan = &mut plans[candidate.deck];
        if candidate.cost <= left[deck.group] && !over_new_limit {
            left[deck.group] -= candidate.cost;
            new_taken[candidate.deck] += u32::from(candidate.unseen);
            plan.today.push(key);
            plan.estimate.today_minutes += candidate.cost / 60.0;
        } else {
            plan.stretch.push(key);
        }
    }
    plans
}

/// Plans every active deadline deck together. Pure: the caller owns all state,
/// and calling this cannot change a card.
pub fn plan(model: &MemoryModel, request: &PlanRequest) -> PlanOutput {
    let last_days: Vec<i32> = request
        .decks
        .iter()
        .map(|deck| last_decision_day(deck.exam_offset_days))
        .collect();
    let days = decision_days(&last_days);
    let outcomes = Outcomes { model, costs: request.costs, ratings: request.ratings };
    let shared = Shared::build(&days, &outcomes);

    let mut tables: Vec<DeckTables> = request
        .decks
        .iter()
        .map(|deck| build_tables(deck, &days, model))
        .collect();
    let entries = build_entries(request, &tables, &days, &outcomes);
    let capacity: Vec<Vec<f32>> = request.groups.iter().map(|&group| capacities(group, &days)).collect();

    let mut prices = vec![vec![MIN_PRICE; days.len()]; request.groups.len()];
    let mut average = vec![vec![0.0_f32; days.len()]; request.groups.len()];
    let mut averaged_totals = vec![Totals::default(); request.decks.len()];
    let averaged_from = ITERATIONS / 2;

    for iteration in 0..ITERATIONS {
        let mut loads = vec![vec![0.0_f32; days.len()]; request.groups.len()];
        for (deck_index, deck) in request.decks.iter().enumerate() {
            let group_prices = &prices[deck.group];
            for tier in 0..5 {
                let in_tier: Vec<&Entry> = entries
                    .iter()
                    .filter(|entry| entry.deck == deck_index && entry.tier == tier)
                    .collect();
                if in_tier.is_empty() {
                    continue;
                }
                solve_tier(&shared, &mut tables[deck_index], tier, group_prices);
                let totals = flow_tier(
                    &shared,
                    &tables[deck_index],
                    tier,
                    &in_tier,
                    group_prices,
                    &mut loads[deck.group],
                );
                if iteration >= averaged_from {
                    averaged_totals[deck_index].add(totals);
                }
            }
        }

        let step = PRICE_STEP / (1.0 + iteration as f32 / 8.0);
        for (group, group_prices) in prices.iter_mut().enumerate() {
            for (epoch, price) in group_prices.iter_mut().enumerate() {
                let room = capacity[group][epoch].max(1.0);
                *price = (*price + step * (loads[group][epoch] - room) / room).max(MIN_PRICE);
                if iteration >= averaged_from {
                    average[group][epoch] += *price / (ITERATIONS - averaged_from) as f32;
                }
            }
        }
    }

    for (deck_index, deck) in request.decks.iter().enumerate() {
        for tier in 0..5 {
            solve_tier(&shared, &mut tables[deck_index], tier, &average[deck.group]);
        }
    }

    let mut decks = rank_today(request, &entries, &tables, &average);
    let samples = (ITERATIONS - averaged_from) as f32;
    for (deck_index, deck) in request.decks.iter().enumerate() {
        let totals = averaged_totals[deck_index];
        let count = deck.cards.len().max(1) as f32;
        let total_weight: f32 = deck.cards.iter().map(|card| f32::from(card.importance.clamp(1, 5))).sum();
        let no_study: f32 = entries
            .iter()
            .filter(|entry| entry.deck == deck_index)
            .map(|entry| entry.members.len() as f32 * weight(entry.tier) * entry.stop_recall)
            .sum();
        let estimate = &mut decks[deck_index].estimate;
        estimate.exam_recall_weighted = totals.weighted_recall / samples / total_weight.max(1.0);
        estimate.exam_recall = totals.recall / samples / count;
        estimate.no_study_recall_weighted = no_study / total_weight.max(1.0);
        estimate.unseen_now = deck.cards.iter().filter(|card| card.memory.is_none()).count();
        estimate.frozen = deck.cards.iter().filter(|card| card.frozen).count();
        estimate.expected_introduced = totals.introduced / samples;
        estimate.expected_reviews = totals.reviews / samples;
        estimate.expected_minutes = totals.seconds / samples / 60.0;
    }
    PlanOutput { day: request.today, decks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(key: &str, importance: u8, stability: f32, last_day: i64) -> PlanCard {
        PlanCard {
            key: key.into(),
            importance,
            memory: Some(Memory { stability, difficulty: 5.0 }),
            last_day: Some(last_day),
            frozen: false,
            position: 0,
        }
    }

    fn unseen(key: &str, importance: u8, position: usize) -> PlanCard {
        PlanCard {
            key: key.into(),
            importance,
            memory: None,
            last_day: None,
            frozen: false,
            position,
        }
    }

    fn deck(uid: &str, days: f64, group: usize, cards: Vec<PlanCard>) -> PlanDeck {
        PlanDeck {
            uid: uid.into(),
            exam_offset_days: days,
            group,
            max_new_today: None,
            cards,
        }
    }

    fn request(decks: Vec<PlanDeck>, groups: Vec<BudgetGroup>) -> PlanRequest {
        PlanRequest {
            today: 100,
            decks,
            groups,
            costs: CostModel::default(),
            ratings: RatingModel::default(),
            seed: 0,
        }
    }

    fn budget(minutes: f32) -> BudgetGroup {
        BudgetGroup { today_s: minutes * 60.0, daily_s: minutes * 60.0 }
    }

    fn run(request: &PlanRequest) -> PlanOutput {
        plan(&MemoryModel::default(), request)
    }

    fn weak_cards(count: usize, importance: u8) -> Vec<PlanCard> {
        (0..count)
            .map(|index| seen(&format!("c{index}"), importance, 3.0, 96))
            .collect()
    }

    #[test]
    fn introduces_unseen_cards_with_time_available() {
        let cards = (0..20).map(|index| unseen(&format!("n{index}"), 3, index)).collect();
        let output = run(&request(vec![deck("a", 14.2, 0, cards)], vec![budget(10.0)]));
        let plan = &output.decks[0];
        assert!(!plan.today.is_empty());
        assert!(plan.estimate.exam_recall_weighted > 0.3, "{:?}", plan.estimate);
        assert!(plan.estimate.expected_introduced > 15.0, "{:?}", plan.estimate);
    }

    #[test]
    fn importance_wins_when_time_is_short() {
        let mut cards = weak_cards(30, 1);
        cards.extend((0..30).map(|index| seen(&format!("vital{index}"), 5, 3.0, 96)));
        let output = run(&request(vec![deck("a", 3.2, 0, cards)], vec![budget(1.5)]));
        let today = &output.decks[0].today;
        let vital = today.iter().filter(|key| key.starts_with("vital")).count();
        assert!(vital * 2 > today.len(), "{today:?}");
    }

    #[test]
    fn today_respects_the_budget_and_lists_each_card_once() {
        let output = run(&request(vec![deck("a", 5.2, 0, weak_cards(100, 3))], vec![budget(2.0)]));
        let today = &output.decks[0].today;
        assert!(today.len() * 12 <= 120 + 12, "{}", today.len());
        let mut unique = today.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), today.len());
    }

    #[test]
    fn decks_in_a_group_share_one_budget() {
        let one = deck("a", 6.2, 0, weak_cards(60, 3));
        let two = deck("b", 9.2, 0, weak_cards(60, 3));
        let output = run(&request(vec![one, two], vec![budget(4.0)]));
        let seconds = (output.decks[0].today.len() + output.decks[1].today.len()) * 12;
        assert!(seconds <= 240, "{seconds}");
    }

    #[test]
    fn a_reserved_group_cannot_spend_anothers_time() {
        let reserved = deck("a", 6.2, 0, weak_cards(60, 3));
        let other = deck("b", 6.2, 1, weak_cards(60, 3));
        let output = run(&request(vec![reserved, other], vec![budget(1.0), budget(4.0)]));
        assert!(output.decks[0].today.len() * 12 <= 60);
        assert!(output.decks[1].today.len() > output.decks[0].today.len());
    }

    #[test]
    fn an_earlier_exam_changes_when_cards_are_reviewed() {
        let cards: Vec<PlanCard> = (0..40).map(|index| seen(&format!("c{index}"), 3, 12.0, 99)).collect();
        let soon = run(&request(vec![deck("a", 1.2, 0, cards.clone())], vec![budget(10.0)]));
        let later = run(&request(vec![deck("a", 40.2, 0, cards)], vec![budget(10.0)]));
        assert!(
            soon.decks[0].today.len() != later.decks[0].today.len(),
            "{} vs {}",
            soon.decks[0].today.len(),
            later.decks[0].today.len()
        );
    }

    #[test]
    fn strong_memories_are_not_all_reviewed_right_before_the_exam() {
        let cards: Vec<PlanCard> = (0..50).map(|index| seen(&format!("c{index}"), 3, 200.0, 99)).collect();
        let output = run(&request(vec![deck("a", 1.2, 0, cards)], vec![budget(30.0)]));
        assert!(output.decks[0].today.len() < 50, "{}", output.decks[0].today.len());
    }

    #[test]
    fn frozen_cards_are_never_planned_but_still_count_in_the_estimate() {
        let mut flagged = unseen("flagged", 5, 0);
        flagged.frozen = true;
        let cards = vec![flagged, seen("other", 3, 5.0, 96)];
        let output = run(&request(vec![deck("a", 7.2, 0, cards)], vec![budget(5.0)]));
        let plan = &output.decks[0];
        assert!(!plan.today.iter().chain(&plan.stretch).any(|key| key == "flagged"));
        assert_eq!(plan.estimate.frozen, 1);
        assert_eq!(plan.estimate.unseen_now, 1);
        assert!(plan.estimate.exam_recall < 0.6, "{:?}", plan.estimate);
    }

    #[test]
    fn a_card_already_reviewed_today_is_not_planned_again_today() {
        let cards = vec![seen("done", 5, 3.0, 100), seen("older", 5, 3.0, 96)];
        let output = run(&request(vec![deck("a", 9.2, 0, cards)], vec![budget(5.0)]));
        assert!(!output.decks[0].today.iter().any(|key| key == "done"));
    }

    #[test]
    fn a_new_card_limit_moves_the_rest_to_stretch() {
        let cards: Vec<PlanCard> = (0..20).map(|index| unseen(&format!("n{index}"), 3, index)).collect();
        let mut limited = deck("a", 4.2, 0, cards);
        limited.max_new_today = Some(3);
        let output = run(&request(vec![limited], vec![budget(30.0)]));
        assert_eq!(output.decks[0].today.len(), 3);
        assert!(!output.decks[0].stretch.is_empty());
    }

    #[test]
    fn a_tighter_budget_predicts_less_recall_and_less_coverage() {
        let cards: Vec<PlanCard> = (0..120).map(|index| unseen(&format!("n{index}"), 3, index)).collect();
        let tight = run(&request(vec![deck("a", 10.2, 0, cards.clone())], vec![budget(2.0)]));
        let ample = run(&request(vec![deck("a", 10.2, 0, cards)], vec![budget(40.0)]));
        let (tight, ample) = (&tight.decks[0].estimate, &ample.decks[0].estimate);
        assert!(tight.exam_recall_weighted < ample.exam_recall_weighted, "{tight:?} {ample:?}");
        assert!(tight.expected_introduced < ample.expected_introduced, "{tight:?} {ample:?}");
    }

    #[test]
    fn a_distant_exam_stays_cheap_to_plan() {
        let days = decision_days(&[200]);
        assert!(days.len() < 28, "{days:?}");
        assert_eq!(days[0], 0);
        assert_eq!(*days.last().unwrap(), 200);
    }
}

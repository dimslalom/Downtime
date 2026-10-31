//! All study behaviour, free of Tauri and of the system clock, so every rule here
//! can be tested with a fixed time. Each operation takes the moment it happened.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::flashcards::{parse_deck, read_library, Card, CardType, Deck, Library, NEUTRAL_IMPORTANCE};

use super::catalog::{match_identity, reconcile};
use super::memory::{
    fit_parameters, training_items, FitOutcome, LearningState, MemoryModel, Moment, Phase, Rating,
    DAY_MS,
};
use super::planner::{
    plan, BudgetGroup, CostModel, DeckEstimate, PlanCard, PlanDeck, PlanOutput, PlanRequest,
    RatingModel,
};
use super::scheduler::{deadline_queue, extra_queue, ordinary_queue, QueueCard};
use super::session::{Attempt, Session, Sessions};
use super::store::{
    CardRecord, Deadline, FolderRecord, LogEntry, ResetEntry, ReviewEntry, Running, SchedulingSettings, Store,
    StudyData, INIT_NO_PRIOR_HISTORY,
};

/// Each measured duration moves the estimate as far as one default observation.
const COST_PRIOR_WEIGHT: f64 = 10.0;
const MEASURED_FROM_REVIEWS: u32 = 20;
const MIN_COST_MS: u64 = 1_000;
const MAX_COST_MS: u64 = 60_000;
const DEFAULT_RELIABILITY: f32 = 0.7;
const RELIABILITY_PRIOR_DAYS: f32 = 5.0;
const RELIABILITY_WINDOW_DAYS: i64 = 14;
const RATING_PRIOR_WEIGHT: f32 = 20.0;
const ORDINARY_LOAD_MAX_SHARE: f32 = 0.8;

fn clear_of_budget(value: f32) -> f32 {
    value.max(1.0)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    Ordinary,
    Deadline,
    Expired,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardView {
    pub attempt_id: String,
    pub key: String,
    pub card_type: Option<CardType>,
    pub front: String,
    pub back: String,
    pub source: Option<String>,
    pub importance: u8,
    pub importance_reason: Option<String>,
    pub flagged: bool,
    pub retry: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    pub deck_id: String,
    pub deck_name: String,
    pub mode: Mode,
    pub extra: bool,
    pub passed: u32,
    pub failed: u32,
    pub remaining: usize,
    pub flipped: bool,
    pub card: Option<CardView>,
    pub next_due_ms: Option<i64>,
    pub plan_pending: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StudyView {
    pub deck_count: usize,
    pub session: Option<SessionView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    /// Model estimates, never guarantees.
    pub estimate: DeckEstimate,
    pub expected_unseen_at_exam: f32,
    pub minutes_available: f32,
    pub minutes_to_cover_unseen: f32,
    pub daily_minutes: f32,
    pub reliability: f32,
    pub target_recall: f32,
    pub fit: Fit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fit {
    Fits,
    Tight,
    Over,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeckSummary {
    pub folder_id: Option<String>,
    pub enabled: bool,
    pub overlay_enabled: bool,
    pub deadline_inherited: bool,
    pub id: String,
    pub name: String,
    pub file: String,
    pub card_count: usize,
    pub seen: usize,
    pub unseen: usize,
    pub due: usize,
    pub flagged: usize,
    pub changed: usize,
    pub orphaned: usize,
    pub mode: Mode,
    pub deadline: Option<Deadline>,
    pub plan: Option<PlanView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CardRow {
    pub key: String,
    pub card_type: Option<CardType>,
    pub front: String,
    pub back: String,
    pub source: Option<String>,
    pub importance: u8,
    pub suggested_importance: Option<u8>,
    pub importance_reason: Option<String>,
    pub assessment_evidence: Option<String>,
    pub flagged: bool,
    pub changed: bool,
    pub phase: Phase,
    pub reviews: u32,
    pub lapses: u32,
    pub recall_now: Option<f32>,
    pub due_ms: Option<i64>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FlaggedCard {
    pub key: String,
    pub deck_name: String,
    pub front: String,
    pub back: String,
    pub source: Option<String>,
    pub flagged_ms: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Measured {
    pub reviews_today: usize,
    pub minutes_today: f32,
    pub daily_minutes: f32,
    pub seconds_per_review: f32,
    pub cost_reviews: u32,
    pub costs_are_measured: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelView {
    pub model_version: String,
    pub personalized: bool,
    pub fitted_ms: Option<i64>,
    pub fit_items: usize,
    pub limited_data: bool,
    pub last_fit_error: Option<String>,
    pub available_items: usize,
    pub review_count: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChangedCard {
    pub key: String,
    pub old_front: String,
    pub old_back: String,
    pub front: String,
    pub back: String,
    pub reviews: u32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub deck_name: String,
    pub file_name: String,
    pub is_update: bool,
    pub total: usize,
    pub new_cards: usize,
    pub unchanged: usize,
    pub edited_unreviewed: usize,
    pub changed_with_history: Vec<ChangedCard>,
    pub removed_with_history: usize,
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum LibraryAction {
    CreateFolder { name: String },
    RenameFolder { id: String, name: String },
    DeleteFolder { id: String },
    MoveDeck { deck: String, folder: Option<String> },
    SelectDeck { deck: String, enabled: bool },
    SelectFolder { id: String, enabled: bool },
    FolderDeadline { id: String, deadline: Deadline },
    DisableFolderDeadline { id: String },
    InheritDeadline { deck: String },
}

pub struct StudyEngine {
    store: Store,
    pub data: StudyData,
    log: HashMap<String, Vec<LogEntry>>,
    reviewed: HashSet<String>,
    next_seq: u64,
    pub model: MemoryModel,
    sessions: Sessions,
    library: Library,
    plan: Option<Arc<PlanOutput>>,
    plan_dirty: bool,
    active: bool,
}

fn replay(
    model: &MemoryModel,
    entries: &[LogEntry],
    desired_retention: f32,
) -> Result<LearningState, String> {
    let mut state = LearningState::default();
    for entry in entries {
        state = match entry {
            LogEntry::Review(review) => {
                model.apply(&state, review.rating, review.at(), desired_retention)?
            }
            LogEntry::Reset(_) => LearningState::default(),
        };
    }
    Ok(state)
}

fn blended(running: Running, default_s: f32) -> f32 {
    let prior = COST_PRIOR_WEIGHT * f64::from(default_s);
    ((prior + running.total_s) / (COST_PRIOR_WEIGHT + f64::from(running.count))) as f32
}

fn deck_key(key: &str) -> String {
    key.split("::").next().unwrap_or(key).to_owned()
}

impl StudyEngine {
    pub fn open(dir: PathBuf, library: Library) -> Result<Self, String> {
        let (store, loaded) = Store::open(dir).map_err(|error| error.to_string())?;
        let mut data = loaded.data;
        let model = MemoryModel::new(&data.model.parameters).unwrap_or_default();
        let next_seq = loaded.log.iter().map(LogEntry::seq).max().unwrap_or(0) + 1;
        let reviewed = loaded
            .log
            .iter()
            .filter_map(|entry| match entry {
                LogEntry::Review(review) => Some(review.id.clone()),
                LogEntry::Reset(_) => None,
            })
            .collect();

        let unapplied: Vec<LogEntry> = loaded
            .log
            .iter()
            .filter(|entry| entry.seq() > data.applied_seq)
            .cloned()
            .collect();
        for entry in &unapplied {
            let retention = data.settings.desired_retention;
            let key = data.resolve(entry.card());
            let record = data.cards.entry(key.clone()).or_insert_with(|| CardRecord {
                deck: deck_key(&key),
                orphaned: true,
                initialization: INIT_NO_PRIOR_HISTORY.into(),
                ..CardRecord::default()
            });
            match entry {
                LogEntry::Review(review) => {
                    let prior = record.learning.clone();
                    record.learning = model.apply(&prior, review.rating, review.at(), retention)?;
                    Self::count_review(&mut data, review, &prior);
                    *data.spent_s.entry(review.at().study_day()).or_default() +=
                        review.active_ms as f64 / 1000.0;
                }
                LogEntry::Reset(_) => record.learning = LearningState::default(),
            }
            data.applied_seq = entry.seq();
        }

        let log = super::store::index_log(&data, loaded.log);
        let mut sessions = loaded.sessions;
        for session in sessions.by_deck.values_mut() {
            if let Some(attempt) = &mut session.current {
                attempt.discard_running_segment();
                // The review was saved just before a crash; the card is done.
                if reviewed_contains(&reviewed, attempt) {
                    session.last_card = Some(attempt.card_key.clone());
                    session.current = None;
                }
            }
        }

        let mut engine = Self {
            store,
            data,
            log,
            reviewed,
            next_seq,
            model,
            sessions,
            library: library.clone(),
            plan: None,
            plan_dirty: true,
            active: false,
        };
        engine.save();
        // The app passes an empty library until the folder has been read. Reconciling
        // that would mark every card as gone.
        if library.decks.is_empty() && library.errors.is_empty() {
            engine.library = library;
        } else {
            engine.library_changed(library, 0);
        }
        Ok(engine)
    }

    fn save(&self) {
        if let Err(error) = self.store.save_state(&self.data) {
            eprintln!("failed to save study state: {error}");
        }
    }

    fn save_sessions(&self) {
        if let Err(error) = self.store.save_sessions(&self.sessions) {
            eprintln!("failed to save sessions: {error}");
        }
    }

    pub fn library(&self) -> &Library {
        &self.library
    }

    pub fn library_changed(&mut self, mut library: Library, now_ms: i64) -> Library {
        for (old, new) in reconcile(&mut self.data, &mut library, now_ms) {
            if let Some(mut entries) = self.log.remove(&old) {
                self.log.entry(new).or_default().append(&mut entries);
            }
        }
        self.library = library;
        // Only auto-file a course once, so moving it back to Unfiled is respected.
        for deck in &self.library.decks {
            if !self.data.deck_folders.contains_key(&deck.id) {
                let course = deck.name.split(" - ").next().unwrap_or("");
                let folder = if course.len() >= 6 && course.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit())
                    && course.chars().any(|c| c.is_ascii_digit()) {
                    let id = course.to_lowercase();
                    self.data.folders.entry(id.clone()).or_insert(FolderRecord {
                        id: id.clone(), name: course.into(), enabled: true, deadline: None,
                    });
                    Some(id)
                } else { None };
                self.data.deck_folders.insert(deck.id.clone(), folder);
            }
        }
        self.plan_dirty = true;
        self.save();
        self.library.clone()
    }

    /// Starts the first session once there is a deck. Existing sessions keep
    /// their snapshots: a changed file applies when they restart.
    pub fn ensure_session(&mut self, at: Moment) {
        if let Some(uid) = self.active_uid() {
            if self.overlay_enabled(&uid) && self.sessions.by_deck.contains_key(&uid) { return; }
            self.interrupt(&uid, at);
            self.sessions.active = None;
        }
        if let Some(uid) = self.library.decks.iter().find(|deck| self.overlay_enabled(&deck.id)).map(|deck| deck.id.clone()) {
            if self.sessions.by_deck.contains_key(&uid) {
                self.sessions.active = Some(uid.clone());
                self.present(&uid, at);
            } else { self.start_session(&uid, false, at); }
        }
        if self.active {
            if let Some(attempt) = self.sessions.active.as_ref().and_then(|uid| self.sessions.by_deck.get_mut(uid)).and_then(|s| s.current.as_mut()) {
                attempt.resume(at.ms);
            }
        }
        self.save_sessions();
    }

    fn start_session(&mut self, uid: &str, extra: bool, at: Moment) {
        let Some(deck) = self.library.decks.iter().find(|deck| deck.id == uid) else {
            return;
        };
        let session = Session {
            deck_uid: deck.id.clone(),
            deck_name: deck.name.clone(),
            day: at.study_day(),
            cards: deck.cards.clone(),
            extra,
            ..Session::default()
        };
        self.sessions.active = Some(uid.to_owned());
        self.sessions.by_deck.insert(uid.to_owned(), session);
        self.present(uid, at);
    }

    fn take_session(&mut self, uid: &str) -> Option<Session> {
        self.sessions.by_deck.remove(uid)
    }

    fn active_uid(&self) -> Option<String> {
        self.sessions.active.clone()
    }

    /// Puts the next card on screen if nothing is, and starts a new session's
    /// snapshot when the study day has turned over.
    fn present(&mut self, uid: &str, at: Moment) {
        let Some(mut session) = self.take_session(uid) else {
            return;
        };
        if session.current.is_none() && session.day != at.study_day() {
            if let Some(deck) = self.library.decks.iter().find(|deck| deck.id == uid) {
                session.cards.clone_from(&deck.cards);
                session.deck_name.clone_from(&deck.name);
            }
            session.day = at.study_day();
            session.passed = 0;
            session.failed = 0;
            session.extra = false;
        }
        if session.current.is_none() {
            if let Some(key) = self.next_key(&session, at) {
                let number = self.data.next_attempt;
                self.data.next_attempt += 1;
                let mut attempt = Attempt {
                    id: format!("a{number}-{}", at.ms),
                    card_key: key,
                    ..Attempt::default()
                };
                if self.active {
                    attempt.resume(at.ms);
                }
                session.current = Some(attempt);
            }
        }
        self.sessions.by_deck.insert(uid.to_owned(), session);
    }

    pub fn mode(&self, uid: &str, at: Moment) -> Mode {
        match self.effective_deadline(uid) {
            Some(deadline) if deadline.enabled && deadline.exam.utc_ms > at.ms => Mode::Deadline,
            Some(deadline) if deadline.enabled => Mode::Expired,
            _ => Mode::Ordinary,
        }
    }

    fn deck_plan(&self, uid: &str, at: Moment) -> Option<&super::planner::DeckPlan> {
        self.plan
            .as_ref()
            .filter(|plan| plan.day == at.study_day())
            .and_then(|plan| plan.decks.iter().find(|deck| deck.uid == uid))
    }

    fn new_allowance(&self, uid: &str, at: Moment) -> u32 {
        let introduced = self
            .data
            .decks
            .get(uid)
            .filter(|deck| deck.new_day == at.study_day())
            .map_or(0, |deck| deck.new_count);
        self.data.settings.new_per_day.saturating_sub(introduced)
    }

    fn queue(&self, session: &Session, at: Moment) -> Vec<String> {
        let keys: Vec<String> = session.cards.iter().map(|card| self.data.resolve(&card.key)).collect();
        let cards: Vec<QueueCard> = keys
            .iter()
            .enumerate()
            .filter_map(|(position, key)| {
                let record = self.data.cards.get(key)?;
                Some(QueueCard {
                    key,
                    state: &record.learning,
                    flagged: record.flagged_ms.is_some(),
                    position,
                })
            })
            .collect();

        let uid = &session.deck_uid;
        let planned = (self.mode(uid, at) == Mode::Deadline)
            .then(|| self.deck_plan(uid, at))
            .flatten();
        let queue: Vec<&str> = match (session.extra, planned) {
            (true, Some(plan)) => {
                let mut queue = deadline_queue(&cards, &plan.stretch, &self.model, at);
                queue.extend(extra_queue(&cards, &self.model, at));
                queue
            }
            (true, None) => extra_queue(&cards, &self.model, at),
            (false, Some(plan)) => deadline_queue(&cards, &plan.today, &self.model, at),
            // Until a plan exists, and whenever deadline mode is off or over.
            (false, None) => ordinary_queue(&cards, &self.model, at, self.new_allowance(uid, at)),
        };
        let mut seen = HashSet::new();
        queue
            .into_iter()
            .filter(|key| seen.insert(*key))
            .map(str::to_owned)
            .collect()
    }

    fn next_key(&self, session: &Session, at: Moment) -> Option<String> {
        let queue = self.queue(session, at);
        queue
            .iter()
            .find(|key| Some(*key) != session.last_card.as_ref())
            .or(queue.first())
            .cloned()
    }

    fn count_review(data: &mut StudyData, review: &ReviewEntry, prior: &LearningState) {
        let index = review.rating as usize - 1;
        let was_new = prior.reps == 0;
        if was_new {
            data.ratings.new[index] += 1;
        } else if prior.phase == Phase::Review {
            data.ratings.review[index] += 1;
        }
        // Only clean timings teach the cost model.
        if review.interruptions == 0
            && (MIN_COST_MS..=MAX_COST_MS).contains(&review.active_ms)
        {
            let seconds = review.active_ms as f64 / 1000.0;
            if was_new {
                data.costs.new.add(seconds);
            } else if review.rating.passed() {
                data.costs.pass.add(seconds);
            } else {
                data.costs.fail.add(seconds);
            }
        }
    }

    fn add_spent(&mut self, at: Moment, added_ms: u64) {
        if added_ms > 0 {
            *self.data.spent_s.entry(at.study_day()).or_default() += added_ms as f64 / 1000.0;
        }
    }

    pub fn flip(&mut self, attempt_id: &str, at: Moment) {
        let Some(uid) = self.active_uid() else { return };
        let Some(session) = self.sessions.by_deck.get_mut(&uid) else { return };
        let Some(attempt) = session.current.as_mut().filter(|attempt| attempt.id == attempt_id) else {
            return;
        };
        attempt.flipped = !attempt.flipped;
        if attempt.flipped {
            attempt.revealed_ms.get_or_insert(at.ms);
        }
        self.save_sessions();
    }

    /// The only path into learning history. A repeat of the same attempt, a stale
    /// attempt, or a card whose answer is not showing changes nothing.
    pub fn grade(&mut self, attempt_id: &str, rating: Rating, at: Moment) -> Result<(), String> {
        if self.reviewed.contains(attempt_id) {
            return Ok(());
        }
        let Some(uid) = self.active_uid() else { return Ok(()) };
        let Some(mut session) = self.take_session(&uid) else { return Ok(()) };
        let result = self.grade_in(&mut session, attempt_id, rating, at);
        self.sessions.by_deck.insert(uid.clone(), session);
        if result.is_ok() {
            self.present(&uid, at);
            self.save();
            self.save_sessions();
        }
        result
    }

    fn grade_in(
        &mut self,
        session: &mut Session,
        attempt_id: &str,
        rating: Rating,
        at: Moment,
    ) -> Result<(), String> {
        let is_ready = session
            .current
            .as_ref()
            .is_some_and(|attempt| attempt.id == attempt_id && attempt.flipped);
        if !is_ready {
            return Ok(());
        }
        let mut attempt = session.current.clone().expect("checked above");
        let key = self.data.resolve(&attempt.card_key);
        let prior = self
            .data
            .cards
            .get(&key)
            .map(|record| record.learning.clone())
            .unwrap_or_default();
        let next = self.model.apply(&prior, rating, at, self.data.settings.desired_retention)?;

        let added = attempt.pause(at.ms);
        let entry = ReviewEntry {
            seq: self.next_seq,
            id: attempt.id.clone(),
            card: key.clone(),
            at_ms: at.ms,
            utc_offset_min: at.utc_offset_min,
            rating,
            prompt_viewed_ms: attempt.prompt_viewed_ms,
            revealed_ms: attempt.revealed_ms,
            active_ms: attempt.active_ms,
            interruptions: attempt.interruptions,
            mode: format!("{:?}", self.mode(&session.deck_uid, at)).to_lowercase(),
        };
        let logged = LogEntry::Review(entry.clone());
        // On failure nothing has changed, so the card can simply be graded again.
        self.store.append(&logged)?;

        self.next_seq += 1;
        self.data.applied_seq = entry.seq;
        self.reviewed.insert(entry.id.clone());
        self.log.entry(key.clone()).or_default().push(logged);
        Self::count_review(&mut self.data, &entry, &prior);
        self.add_spent(at, added);

        if let Some(record) = self.data.cards.get_mut(&key) {
            record.learning = next;
        }
        if prior.reps == 0 {
            if let Some(deck) = self.data.decks.get_mut(&session.deck_uid) {
                if deck.new_day != at.study_day() {
                    deck.new_day = at.study_day();
                    deck.new_count = 0;
                }
                deck.new_count += 1;
            }
        }
        session.passed += u32::from(rating.passed());
        session.failed += u32::from(!rating.passed());
        session.last_card = Some(key);
        session.current = None;
        self.plan_dirty = true;
        Ok(())
    }

    /// Overlay opened and focused, or hidden, blurred or switched away. Nothing
    /// here can reach the review log: leaving a card is never a grade.
    pub fn set_active(&mut self, active: bool, at: Moment) {
        if self.active == active {
            return;
        }
        self.active = active;
        if active {
            self.ensure_session(at);
        }
        let Some(uid) = self.active_uid() else { return };
        if active {
            self.present(&uid, at);
            if let Some(attempt) = self
                .sessions
                .by_deck
                .get_mut(&uid)
                .and_then(|session| session.current.as_mut())
            {
                attempt.resume(at.ms);
            }
        } else {
            self.interrupt(&uid, at);
        }
        self.save_sessions();
    }

    /// Peek's pause after a grade. The next card exists but is not on screen, so
    /// its attempt has not started: no clock, and hiding is no interruption.
    pub fn rest(&mut self) {
        let Some(attempt) = self.current_attempt_mut() else { return };
        if attempt.active_ms == 0 && attempt.revealed_ms.is_none() {
            attempt.resumed_ms = None;
            attempt.prompt_viewed_ms = None;
        }
        self.save_sessions();
    }

    /// The player asked for the next card during that pause.
    pub fn end_rest(&mut self, at: Moment) {
        if !self.active {
            return;
        }
        let Some(attempt) = self.current_attempt_mut() else { return };
        attempt.resume(at.ms);
        self.save_sessions();
    }

    fn current_attempt_mut(&mut self) -> Option<&mut Attempt> {
        let uid = self.active_uid()?;
        self.sessions.by_deck.get_mut(&uid)?.current.as_mut()
    }

    fn interrupt(&mut self, uid: &str, at: Moment) {
        let Some(attempt) = self
            .sessions
            .by_deck
            .get_mut(uid)
            .and_then(|session| session.current.as_mut())
            .filter(|attempt| attempt.resumed_ms.is_some())
        else {
            return;
        };
        let added = attempt.pause(at.ms);
        attempt.interruptions += 1;
        let key = attempt.card_key.clone();
        self.add_spent(at, added);
        let key = self.data.resolve(&key);
        if let Some(record) = self.data.cards.get_mut(&key) {
            record.interruptions += 1;
        }
        self.save();
    }

    pub fn next_deck(&mut self, at: Moment) {
        let decks: Vec<String> = self.library.decks.iter().filter(|deck| self.overlay_enabled(&deck.id)).map(|deck| deck.id.clone()).collect();
        if decks.is_empty() { return; }
        let index = self
            .sessions
            .active
            .as_ref()
            .and_then(|uid| decks.iter().position(|deck| deck == uid));
        let Some(next) = decks.get(index.map_or(0, |index| (index + 1) % decks.len())).cloned() else {
            return;
        };
        if self.sessions.active.as_ref() == Some(&next) {
            return;
        }
        if let Some(uid) = self.active_uid() {
            self.interrupt(&uid, at);
        }
        if self.sessions.by_deck.contains_key(&next) {
            self.sessions.active = Some(next.clone());
            self.present(&next, at);
        } else {
            self.start_session(&next, false, at);
        }
        if self.active {
            if let Some(attempt) = self
                .sessions
                .by_deck
                .get_mut(&next)
                .and_then(|session| session.current.as_mut())
            {
                attempt.resume(at.ms);
            }
        }
        self.save();
        self.save_sessions();
    }

    /// Space on a finished view: pick up the latest cards, and if nothing is
    /// scheduled, carry on with extra practice because the player asked to.
    pub fn restart(&mut self, at: Moment) {
        let Some(uid) = self.active_uid() else {
            self.ensure_session(at);
            return;
        };
        let is_finished = self
            .sessions
            .by_deck
            .get(&uid)
            .is_none_or(|session| session.current.is_none());
        if !is_finished {
            return;
        }
        self.start_session(&uid, false, at);
        let nothing_scheduled = self
            .sessions
            .by_deck
            .get(&uid)
            .is_some_and(|session| session.current.is_none());
        if nothing_scheduled {
            self.start_session(&uid, true, at);
        }
        self.save();
        self.save_sessions();
    }

    /// Sets a card aside for focused study. It is not a grade, so its memory
    /// state and history stay exactly as they were.
    pub fn flag_current(&mut self, attempt_id: &str, at: Moment) {
        let Some(uid) = self.active_uid() else { return };
        let Some(mut session) = self.take_session(&uid) else { return };
        let matching = session.current.as_ref().is_some_and(|attempt| attempt.id == attempt_id);
        if matching {
            let mut attempt = session.current.take().expect("checked above");
            let key = self.data.resolve(&attempt.card_key);
            let was_flagged = self
                .data
                .cards
                .get(&key)
                .is_some_and(|record| record.flagged_ms.is_some());
            if was_flagged {
                self.set_flag(&key, false, at);
                session.current = Some(attempt);
            } else {
                let added = attempt.pause(at.ms);
                self.add_spent(at, added);
                self.set_flag(&key, true, at);
                session.last_card = Some(key);
            }
        }
        self.sessions.by_deck.insert(uid.clone(), session);
        self.present(&uid, at);
        self.save();
        self.save_sessions();
    }

    pub fn set_flag(&mut self, key: &str, flagged: bool, at: Moment) {
        let key = self.data.resolve(key);
        if let Some(record) = self.data.cards.get_mut(&key) {
            record.flagged_ms = flagged.then_some(at.ms);
            self.plan_dirty = true;
            self.save();
        }
    }

    pub fn set_importance(&mut self, key: &str, importance: Option<u8>) -> Result<(), String> {
        if importance.is_some_and(|value| !(1..=5).contains(&value)) {
            return Err("Importance must be from 1 to 5.".into());
        }
        let key = self.data.resolve(key);
        let record = self.data.cards.get_mut(&key).ok_or("That card no longer exists.")?;
        record.importance_override = importance;
        self.plan_dirty = true;
        self.save();
        Ok(())
    }

    pub fn keep_history(&mut self, key: &str) {
        let key = self.data.resolve(key);
        if let Some(record) = self.data.cards.get_mut(&key) {
            record.content_changed = false;
            self.save();
        }
    }

    /// For a card that now tests something else. The old reviews stay in the
    /// log, marked by this reset, but stop informing its memory state.
    pub fn reset_card(&mut self, key: &str, reason: &str, at: Moment) -> Result<(), String> {
        let key = self.data.resolve(key);
        if !self.data.cards.contains_key(&key) {
            return Err("That card no longer exists.".into());
        }
        let entry = LogEntry::Reset(ResetEntry {
            seq: self.next_seq,
            card: key.clone(),
            at_ms: at.ms,
            reason: reason.to_owned(),
        });
        self.store.append(&entry)?;
        self.next_seq += 1;
        self.data.applied_seq = entry.seq();
        self.log.entry(key.clone()).or_default().push(entry);
        if let Some(record) = self.data.cards.get_mut(&key) {
            record.learning = LearningState::default();
            record.content_changed = false;
        }
        self.plan_dirty = true;
        self.save();
        Ok(())
    }

    pub fn set_settings(&mut self, settings: SchedulingSettings) -> Result<(), String> {
        if !(0.7..=0.97).contains(&settings.desired_retention) {
            return Err("Desired retention must be between 70% and 97%.".into());
        }
        if !(1.0..=600.0).contains(&settings.daily_minutes) {
            return Err("Daily study time must be between 1 and 600 minutes.".into());
        }
        if settings.new_per_day > 1000 {
            return Err("New cards per day must be 1000 or fewer.".into());
        }
        self.data.settings = settings;
        let retention = self.data.settings.desired_retention;
        for record in self.data.cards.values_mut() {
            if record.learning.phase == Phase::Review {
                record.learning.due_day = self.model.due_day_for(&record.learning, retention);
            }
        }
        self.plan_dirty = true;
        self.save();
        Ok(())
    }

    fn validate_deadline(deadline: &Deadline, at: Moment) -> Result<(), String> {
        if deadline.enabled && deadline.exam.utc_ms <= at.ms {
            return Err("Choose an exam time in the future.".into());
        }
        if deadline.exam.time_zone.is_empty() {
            return Err("The exam time needs a time zone.".into());
        }
        if deadline.daily_minutes.is_some_and(|minutes| !(1.0..=600.0).contains(&minutes)) {
            return Err("Minutes for this deck must be between 1 and 600.".into());
        }
        let advanced = &deadline.advanced;
        if advanced.reliability.is_some_and(|value| !(0.1..=1.0).contains(&value)) {
            return Err("Reliability must be between 10% and 100%.".into());
        }
        if advanced.target_recall.is_some_and(|value| !(0.5..=0.99).contains(&value)) {
            return Err("Target recall must be between 50% and 99%.".into());
        }
        Ok(())
    }

    pub fn folders(&self) -> Vec<FolderRecord> {
        self.data.folders.values().cloned().collect()
    }

    fn folder(&self, uid: &str) -> Option<&FolderRecord> {
        self.data.deck_folders.get(uid).and_then(|id| id.as_ref()).and_then(|id| self.data.folders.get(id))
    }

    pub fn overlay_enabled(&self, uid: &str) -> bool {
        self.library.decks.iter().any(|d| d.id == uid)
            && !self.data.disabled_decks.contains(uid)
            && self.folder(uid).is_none_or(|f| f.enabled)
    }

    fn effective_deadline(&self, uid: &str) -> Option<&Deadline> {
        self.data.decks.get(uid)?.deadline.as_ref().or_else(|| self.folder(uid)?.deadline.as_ref())
    }

    pub fn organize(&mut self, action: LibraryAction, at: Moment) -> Result<(), String> {
        let name = |n: String| -> Result<String, String> {
            let n = n.trim();
            if n.is_empty() || n.chars().count() > 100 { return Err("Use a folder name of 1 to 100 characters.".into()); }
            Ok(n.into())
        };
        match action {
            LibraryAction::CreateFolder { name: n } => {
                let n = name(n)?;
                if self.data.folders.values().any(|f| f.name.eq_ignore_ascii_case(&n)) { return Err("A folder with that name already exists.".into()); }
                let id = (1..).map(|i| format!("folder-{i}")).find(|id| !self.data.folders.contains_key(id)).unwrap();
                self.data.folders.insert(id.clone(), FolderRecord { id, name: n, enabled: true, deadline: None });
            }
            LibraryAction::RenameFolder { id, name: n } => {
                let n = name(n)?;
                if self.data.folders.values().any(|f| f.id != id && f.name.eq_ignore_ascii_case(&n)) { return Err("A folder with that name already exists.".into()); }
                self.data.folders.get_mut(&id).ok_or("Folder not found.")?.name = n;
            }
            LibraryAction::DeleteFolder { id } => {
                self.data.folders.remove(&id).ok_or("Folder not found.")?;
                for folder in self.data.deck_folders.values_mut() { if folder.as_ref() == Some(&id) { *folder = None; } }
            }
            LibraryAction::MoveDeck { deck, folder } => {
                if !self.data.decks.contains_key(&deck) { return Err("Deck not found.".into()); }
                if folder.as_ref().is_some_and(|f| !self.data.folders.contains_key(f)) { return Err("Folder not found.".into()); }
                self.data.deck_folders.insert(deck, folder);
            }
            LibraryAction::SelectDeck { deck, enabled } => {
                if !self.data.decks.contains_key(&deck) { return Err("Deck not found.".into()); }
                if enabled { self.data.disabled_decks.remove(&deck); } else { self.data.disabled_decks.insert(deck); }
            }
            LibraryAction::SelectFolder { id, enabled } => { self.data.folders.get_mut(&id).ok_or("Folder not found.")?.enabled = enabled; }
            LibraryAction::FolderDeadline { id, deadline } => {
                Self::validate_deadline(&deadline, at)?;
                self.data.folders.get_mut(&id).ok_or("Folder not found.")?.deadline = Some(deadline);
            }
            LibraryAction::DisableFolderDeadline { id } => {
                if let Some(d) = self.data.folders.get_mut(&id).ok_or("Folder not found.")?.deadline.as_mut() { d.enabled = false; }
            }
            LibraryAction::InheritDeadline { deck } => { self.data.decks.get_mut(&deck).ok_or("Deck not found.")?.deadline = None; }
        }
        self.plan = None;
        self.plan_dirty = true;
        self.ensure_session(at);
        self.store.save_state(&self.data)?;
        Ok(())
    }

    pub fn set_deadline(&mut self, uid: &str, deadline: Deadline, at: Moment) -> Result<(), String> {
        Self::validate_deadline(&deadline, at)?;
        let deck = self.data.decks.get_mut(uid).ok_or("That deck no longer exists.")?;
        deck.deadline = Some(deadline);
        self.plan = None;
        self.plan_dirty = true;
        self.save();
        Ok(())
    }

    /// Turns deadline mode off and keeps the date for later. Cards were never
    /// touched by the plan, so their ordinary FSRS due dates are already right.
    pub fn disable_deadline(&mut self, uid: &str) {
        if self.data.decks.get(uid).is_some_and(|d| d.deadline.is_none()) {
            let inherited = self.effective_deadline(uid).cloned();
            if let Some(deck) = self.data.decks.get_mut(uid) { deck.deadline = inherited; }
        }
        if let Some(deadline) = self
            .data
            .decks
            .get_mut(uid)
            .and_then(|deck| deck.deadline.as_mut())
        {
            deadline.enabled = false;
            self.plan_dirty = true;
            self.save();
        }
    }

    pub fn view(&self, at: Moment) -> StudyView {
        let deck_count = self.library.decks.iter().filter(|deck| self.overlay_enabled(&deck.id)).count();
        let session = self
            .sessions
            .active
            .as_ref()
            .and_then(|uid| self.sessions.by_deck.get(uid))
            .map(|session| self.session_view(session, at));
        StudyView { deck_count, session }
    }

    fn effective_importance(&self, card: &Card, record: Option<&CardRecord>) -> u8 {
        record
            .and_then(|record| record.importance_override)
            .or(card.importance)
            .unwrap_or(NEUTRAL_IMPORTANCE)
    }

    fn next_due(&self, session: &Session, at: Moment) -> Option<i64> {
        session
            .cards
            .iter()
            .filter_map(|card| self.data.cards.get(&self.data.resolve(&card.key)))
            .filter(|record| record.flagged_ms.is_none())
            .filter_map(|record| match record.learning.phase {
                Phase::New => None,
                Phase::Learning | Phase::Relearning => record.learning.due_ms,
                Phase::Review => record.learning.due_day.map(|day| at.day_start_ms(day)),
            })
            .filter(|&due| due > at.ms)
            .min()
    }

    fn session_view(&self, session: &Session, at: Moment) -> SessionView {
        let mode = self.mode(&session.deck_uid, at);
        let card = session.current.as_ref().and_then(|attempt| {
            let key = self.data.resolve(&attempt.card_key);
            let card = session
                .cards
                .iter()
                .find(|card| self.data.resolve(&card.key) == key)?;
            let record = self.data.cards.get(&key);
            Some(CardView {
                retry: matches!(self.log.get(&key).and_then(|entries| entries.last()), Some(LogEntry::Review(review)) if !review.rating.passed()),
                attempt_id: attempt.id.clone(),
                key,
                card_type: card.card_type,
                front: card.front.clone(),
                back: card.back.clone(),
                source: card.source.clone(),
                importance: self.effective_importance(card, record),
                importance_reason: card.importance_reason.clone(),
                flagged: record.is_some_and(|record| record.flagged_ms.is_some()),
            })
        });
        SessionView {
            deck_id: session.deck_uid.clone(),
            deck_name: session.deck_name.clone(),
            mode,
            extra: session.extra,
            passed: session.passed,
            failed: session.failed,
            remaining: self.queue(session, at).len(),
            flipped: session.current.as_ref().is_some_and(|attempt| attempt.flipped),
            card,
            next_due_ms: self.next_due(session, at),
            plan_pending: mode == Mode::Deadline && self.deck_plan(&session.deck_uid, at).is_none(),
        }
    }

    fn costs(&self) -> CostModel {
        let defaults = CostModel::default();
        let costs = &self.data.costs;
        CostModel {
            new_s: blended(costs.new, defaults.new_s),
            pass_s: blended(costs.pass, defaults.pass_s),
            fail_s: blended(costs.fail, defaults.fail_s),
        }
    }

    fn rating_shares(&self) -> RatingModel {
        let defaults = RatingModel::default();
        let shares = |counts: &[u32], prior: &[f32]| -> Vec<f32> {
            let total: f32 = counts.iter().map(|&count| count as f32).sum();
            counts
                .iter()
                .zip(prior)
                .map(|(&count, &share)| {
                    (count as f32 + share * RATING_PRIOR_WEIGHT) / (total + RATING_PRIOR_WEIGHT)
                })
                .collect()
        };
        let review = shares(&self.data.ratings.review[1..], &defaults.review);
        let new = shares(&self.data.ratings.new, &defaults.new);
        RatingModel {
            review: [review[0], review[1], review[2]],
            new: [new[0], new[1], new[2], new[3]],
        }
    }

    fn reliability(&self, deck_override: Option<f32>, today: i64) -> f32 {
        if let Some(value) = deck_override {
            return value;
        }
        let Some((&first, _)) = self.data.spent_s.iter().next() else {
            return DEFAULT_RELIABILITY;
        };
        let daily = self.data.settings.daily_minutes * 60.0;
        let start = first.max(today - RELIABILITY_WINDOW_DAYS);
        let days = (today - start).max(0) as f32;
        let used: f32 = (start..today)
            .map(|day| {
                let spent = self.data.spent_s.get(&day).copied().unwrap_or(0.0) as f32;
                (spent / daily).min(1.0)
            })
            .sum();
        ((DEFAULT_RELIABILITY * RELIABILITY_PRIOR_DAYS + used) / (RELIABILITY_PRIOR_DAYS + days))
            .clamp(0.1, 1.0)
    }

    /// Reviews ordinary decks can be expected to want, so that planned decks
    /// are not promised time those will use.
    fn ordinary_load_s(&self, at: Moment) -> (f32, f32) {
        let pass = self.costs().pass_s;
        let (mut today, mut daily) = (0.0, 0.0);
        for record in self.data.cards.values() {
            if !self.overlay_enabled(&record.deck) || record.orphaned || self.mode(&record.deck, at) == Mode::Deadline {
                continue;
            }
            if record.learning.phase != Phase::Review || record.flagged_ms.is_some() {
                continue;
            }
            if record.learning.is_due(at) {
                today += pass;
            }
            if let (Some(due), Some(last)) = (record.learning.due_day, record.learning.last_review_day) {
                daily += pass / (due - last).max(1) as f32;
            }
        }
        (today, daily)
    }

    fn deadline_decks(&self, at: Moment) -> Vec<&Deck> {
        self.library
            .decks
            .iter()
            .filter(|deck| self.overlay_enabled(&deck.id) && self.mode(&deck.id, at) == Mode::Deadline)
            .collect()
    }

    /// Splits the shared estimate. Reservations come out first, scaled down if
    /// they promise more than exists, and the rest is one pool.
    fn budget_groups(&self, at: Moment) -> (Vec<BudgetGroup>, HashMap<String, usize>) {
        let today = at.study_day();
        let shared = self.data.settings.daily_minutes * 60.0;
        let decks = self.deadline_decks(at);
        let reserved_decks: Vec<(&str, f32)> = decks
            .iter()
            .filter_map(|deck| {
                let minutes = self.effective_deadline(&deck.id)?.daily_minutes?;
                Some((deck.id.as_str(), minutes * 60.0))
            })
            .collect();
        let mut reservations: std::collections::BTreeMap<String, (f32, Vec<String>)> = Default::default();
        for (uid, seconds) in reserved_decks {
            let key = if self.data.decks.get(uid).is_some_and(|r| r.deadline.is_none()) {
                format!("folder:{}", self.data.deck_folders.get(uid).and_then(|f| f.as_deref()).unwrap_or(uid))
            } else { format!("deck:{uid}") };
            reservations.entry(key).or_insert((seconds, Vec::new())).1.push(uid.to_owned());
        }
        let reserved: Vec<(Vec<String>, f32)> = reservations.into_values().map(|(s, ids)| (ids, s)).collect();
        let promised: f32 = reserved.iter().map(|(_, seconds)| seconds).sum();
        let scale = if promised > shared { shared / promised } else { 1.0 };

        let spent = self.data.spent_s.get(&today).copied().unwrap_or(0.0) as f32;
        let left_share = ((shared - spent) / clear_of_budget(shared)).clamp(0.0, 1.0);
        let reliability = decks
            .iter()
            .map(|deck| {
                let chosen = self.effective_deadline(&deck.id)
                    .and_then(|deadline| deadline.advanced.reliability);
                self.reliability(chosen, today)
            })
            .fold(1.0, f32::min);

        let mut groups = Vec::new();
        let mut membership = HashMap::new();
        for (uids, seconds) in &reserved {
            for uid in uids { membership.insert(uid.clone(), groups.len()); }
            let daily = *seconds * scale;
            groups.push(BudgetGroup {
                today_s: daily * left_share,
                daily_s: daily * reliability,
            });
        }
        if decks.iter().any(|deck| !membership.contains_key(&deck.id)) {
            let pool = (shared - promised * scale).max(0.0);
            let (ordinary_today, ordinary_daily) = self.ordinary_load_s(at);
            let group = groups.len();
            for deck in decks.iter().filter(|deck| !membership.contains_key(&deck.id)).collect::<Vec<_>>() {
                membership.insert(deck.id.clone(), group);
            }
            groups.push(BudgetGroup {
                today_s: (pool * left_share - ordinary_today).max(pool * left_share * (1.0 - ORDINARY_LOAD_MAX_SHARE)),
                daily_s: ((pool * reliability) - ordinary_daily)
                    .max(pool * reliability * (1.0 - ORDINARY_LOAD_MAX_SHARE)),
            });
        }
        (groups, membership)
    }

    pub fn plan_request(&self, at: Moment) -> Option<PlanRequest> {
        let today = at.study_day();
        let start = at.day_start_ms(today);
        let (groups, membership) = self.budget_groups(at);
        let decks: Vec<PlanDeck> = self
            .deadline_decks(at)
            .into_iter()
            .filter_map(|deck| {
                let deadline = self.effective_deadline(&deck.id)?;
                let cards = deck
                    .cards
                    .iter()
                    .enumerate()
                    .map(|(position, card)| {
                        let record = self.data.cards.get(&card.key);
                        let memory = record.and_then(|record| record.learning.memory);
                        PlanCard {
                            key: card.key.clone(),
                            importance: self.effective_importance(card, record),
                            memory,
                            last_day: record.and_then(|record| record.learning.last_review_day),
                            frozen: record.is_some_and(|record| record.flagged_ms.is_some()),
                            position,
                        }
                    })
                    .collect();
                Some(PlanDeck {
                    uid: deck.id.clone(),
                    exam_offset_days: (deadline.exam.utc_ms - start) as f64 / DAY_MS as f64,
                    group: *membership.get(&deck.id)?,
                    max_new_today: deadline.advanced.max_new_per_day,
                    cards,
                })
            })
            .collect();
        (!decks.is_empty()).then(|| PlanRequest {
            today,
            decks,
            groups,
            costs: self.costs(),
            ratings: self.rating_shares(),
            seed: 0,
        })
    }

    /// Whether the background planner has work: something changed, or a new day
    /// began, which also accounts for any study days that were missed.
    pub fn needs_plan(&self, at: Moment) -> bool {
        let is_stale = self.plan.as_ref().is_none_or(|plan| plan.day != at.study_day());
        !self.deadline_decks(at).is_empty() && (self.plan_dirty || is_stale)
    }

    pub fn take_plan_request(&mut self, at: Moment) -> Option<(PlanRequest, MemoryModel)> {
        self.plan_dirty = false;
        let request = self.plan_request(at)?;
        Some((request, self.model.clone()))
    }

    pub fn install_plan(&mut self, output: PlanOutput, at: Moment) {
        self.plan = Some(Arc::new(output));
        if let Some(uid) = self.active_uid() {
            self.present(&uid, at);
            self.save_sessions();
        }
    }

    /// Computes the plan inline. Only for tests and the offline comparison:
    /// the app does this on its planning thread.
    pub fn replan_now(&mut self, at: Moment) {
        match self.take_plan_request(at) {
            Some((request, model)) => {
                let output = plan(&model, &request);
                self.install_plan(output, at);
            }
            None => self.plan = None,
        }
    }

    fn plan_view(&self, uid: &str, at: Moment) -> Option<PlanView> {
        if self.mode(uid, at) != Mode::Deadline {
            return None;
        }
        let plan = self.deck_plan(uid, at)?;
        let deadline = self.effective_deadline(uid)?;
        let (groups, membership) = self.budget_groups(at);
        let group = groups.get(*membership.get(uid)?)?;
        let start = at.day_start_ms(at.study_day());
        let days_left = ((deadline.exam.utc_ms - start) as f32 / DAY_MS as f32 - 0.5).max(0.0);
        let minutes_available = (group.today_s + group.daily_s * (days_left - 1.0).max(0.0)) / 60.0;
        let estimate = plan.estimate.clone();
        let unseen_at_exam = (estimate.unseen_now as f32 - estimate.expected_introduced).max(0.0);
        let minutes_to_cover = estimate.unseen_now as f32 * self.costs().new_s / 60.0;
        let target_recall = deadline
            .advanced
            .target_recall
            .unwrap_or(self.data.settings.desired_retention);
        let total = self.library.decks.iter().find(|deck| deck.id == uid)?.cards.len() as f32;

        let fit = if minutes_to_cover > minutes_available || unseen_at_exam > (total * 0.01).max(0.5) {
            Fit::Over
        } else if estimate.exam_recall_weighted < target_recall {
            Fit::Tight
        } else {
            Fit::Fits
        };
        Some(PlanView {
            expected_unseen_at_exam: unseen_at_exam,
            minutes_available,
            minutes_to_cover_unseen: minutes_to_cover,
            daily_minutes: group.daily_s / 60.0,
            reliability: self.reliability(deadline.advanced.reliability, at.study_day()),
            target_recall,
            fit,
            estimate,
        })
    }

    pub fn deck_summaries(&self, at: Moment) -> Vec<DeckSummary> {
        self.library
            .decks
            .iter()
            .map(|deck| {
                let records: Vec<&CardRecord> = deck
                    .cards
                    .iter()
                    .filter_map(|card| self.data.cards.get(&card.key))
                    .collect();
                let count = |test: &dyn Fn(&CardRecord) -> bool| {
                    records.iter().filter(|record| test(record)).count()
                };
                DeckSummary {
                    id: deck.id.clone(),
                    name: deck.name.clone(),
                    file: deck.file.clone(),
                    card_count: deck.cards.len(),
                    seen: count(&|record| record.learning.reps > 0),
                    unseen: count(&|record| record.learning.reps == 0),
                    due: count(&|record| record.learning.is_due(at)),
                    flagged: count(&|record| record.flagged_ms.is_some()),
                    changed: count(&|record| record.content_changed),
                    orphaned: self
                        .data
                        .cards
                        .values()
                        .filter(|record| record.deck == deck.id && record.orphaned && record.learning.reps > 0)
                        .count(),
                    mode: self.mode(&deck.id, at),
                    deadline: self.effective_deadline(&deck.id).cloned(),
                    folder_id: self.data.deck_folders.get(&deck.id).cloned().flatten(),
                    enabled: !self.data.disabled_decks.contains(&deck.id),
                    overlay_enabled: self.overlay_enabled(&deck.id),
                    deadline_inherited: self.data.decks.get(&deck.id).is_some_and(|r| r.deadline.is_none()) && self.effective_deadline(&deck.id).is_some(),
                    plan: self.plan_view(&deck.id, at),
                }
            })
            .collect()
    }

    pub fn deck_cards(&self, uid: &str, at: Moment) -> Vec<CardRow> {
        let Some(deck) = self.library.decks.iter().find(|deck| deck.id == uid) else {
            return Vec::new();
        };
        deck.cards
            .iter()
            .map(|card| {
                let record = self.data.cards.get(&card.key);
                let learning = record.map(|record| record.learning.clone()).unwrap_or_default();
                let recall_now = learning.memory.zip(learning.last_review_ms).map(|(memory, last)| {
                    self.model.retrievability(memory, (at.ms - last) as f32 / DAY_MS as f32)
                });
                CardRow {
                    key: card.key.clone(),
                    card_type: card.card_type,
                    front: card.front.clone(),
                    back: card.back.clone(),
                    source: card.source.clone(),
                    importance: self.effective_importance(card, record),
                    suggested_importance: card.importance,
                    importance_reason: card.importance_reason.clone(),
                    assessment_evidence: card.assessment_evidence.clone(),
                    flagged: record.is_some_and(|record| record.flagged_ms.is_some()),
                    changed: record.is_some_and(|record| record.content_changed),
                    phase: learning.phase,
                    reviews: learning.reps,
                    lapses: learning.lapses,
                    recall_now,
                    due_ms: match learning.phase {
                        Phase::New => None,
                        Phase::Review => learning.due_day.map(|day| at.day_start_ms(day)),
                        _ => learning.due_ms,
                    },
                }
            })
            .collect()
    }

    pub fn flagged_cards(&self) -> Vec<FlaggedCard> {
        let mut flagged: Vec<FlaggedCard> = self
            .library
            .decks
            .iter()
            .flat_map(|deck| deck.cards.iter().map(move |card| (deck, card)))
            .filter_map(|(deck, card)| {
                let flagged_ms = self.data.cards.get(&card.key)?.flagged_ms?;
                Some(FlaggedCard {
                    key: card.key.clone(),
                    deck_name: deck.name.clone(),
                    front: card.front.clone(),
                    back: card.back.clone(),
                    source: card.source.clone(),
                    flagged_ms,
                })
            })
            .collect();
        flagged.sort_by_key(|card| card.flagged_ms);
        flagged
    }

    pub fn measured(&self, at: Moment) -> Measured {
        let today = at.study_day();
        let costs = &self.data.costs;
        let cost_reviews = costs.new.count + costs.pass.count + costs.fail.count;
        let reviews_today = self
            .log
            .values()
            .flatten()
            .filter(|entry| matches!(entry, LogEntry::Review(review) if review.at().study_day() == today))
            .count();
        Measured {
            reviews_today,
            minutes_today: self.data.spent_s.get(&today).copied().unwrap_or(0.0) as f32 / 60.0,
            daily_minutes: self.data.settings.daily_minutes,
            seconds_per_review: self.costs().pass_s,
            cost_reviews,
            costs_are_measured: cost_reviews >= MEASURED_FROM_REVIEWS,
        }
    }

    pub fn settings(&self) -> SchedulingSettings {
        self.data.settings.clone()
    }

    fn training_set(&self) -> Vec<fsrs::FSRSItem> {
        let mut items = Vec::new();
        for entries in self.log.values() {
            let mut segment = Vec::new();
            for entry in entries {
                match entry {
                    LogEntry::Review(review) => segment.push((review.at().study_day(), review.rating)),
                    LogEntry::Reset(_) => {
                        items.extend(training_items(&segment));
                        segment.clear();
                    }
                }
            }
            items.extend(training_items(&segment));
        }
        items
    }

    pub fn model_view(&self) -> ModelView {
        let record = &self.data.model;
        ModelView {
            model_version: record.model_version.clone(),
            personalized: record.fitted,
            fitted_ms: record.fitted_ms,
            fit_items: record.fit_items,
            limited_data: record.limited_data,
            last_fit_error: record.last_fit_error.clone(),
            available_items: self.training_set().len(),
            review_count: self.reviewed.len(),
        }
    }

    /// Snapshot for the optimizer, which runs off the review path.
    pub fn fit_input(&self) -> Vec<fsrs::FSRSItem> {
        self.training_set()
    }

    /// Adopts a fit, or keeps the current parameters and says why. Memory is
    /// replayed from the raw log under the new parameters.
    pub fn apply_fit(&mut self, result: Result<FitOutcome, String>, items: usize, at: Moment) {
        match result {
            Ok(FitOutcome::Fitted { parameters, limited_data }) => match MemoryModel::new(&parameters) {
                Ok(model) => {
                    let replayed = self.replayed_with(&model);
                    match replayed {
                        Ok(states) => {
                            for (key, state) in states {
                                if let Some(record) = self.data.cards.get_mut(&key) {
                                    record.learning = state;
                                }
                            }
                            let record = &mut self.data.model;
                            record.previous_parameters =
                                Some(std::mem::replace(&mut record.parameters, parameters));
                            record.fitted = true;
                            record.fitted_ms = Some(at.ms);
                            record.fit_items = items;
                            record.limited_data = limited_data;
                            record.last_fit_error = None;
                            self.model = model;
                            self.plan_dirty = true;
                        }
                        Err(error) => self.data.model.last_fit_error = Some(error),
                    }
                }
                Err(error) => self.data.model.last_fit_error = Some(error),
            },
            Ok(FitOutcome::NotEnoughData) => {
                self.data.model.last_fit_error =
                    Some("Not enough delayed reviews yet. The current parameters stay in use.".into());
            }
            Err(error) => {
                self.data.model.last_fit_error =
                    Some(format!("Fitting failed, so the current parameters stay in use: {error}"));
            }
        }
        self.save();
    }

    fn replayed_with(&self, model: &MemoryModel) -> Result<Vec<(String, LearningState)>, String> {
        let retention = self.data.settings.desired_retention;
        self.log
            .iter()
            .map(|(key, entries)| Ok((key.clone(), replay(model, entries, retention)?)))
            .collect()
    }

    pub fn fit_blocking(&mut self, at: Moment) {
        let items = self.fit_input();
        let count = items.len();
        let result = fit_parameters(items);
        self.apply_fit(result, count, at);
    }

    fn import_target(&self, deck: &Deck) -> (Option<String>, String) {
        let present: HashSet<String> = self
            .library
            .decks
            .iter()
            .map(|deck| deck.file.to_lowercase())
            .collect();
        let by_name = self
            .data
            .decks
            .values()
            .find(|record| record.name.eq_ignore_ascii_case(&deck.name));
        if let Some(record) = by_name {
            return (Some(record.uid.clone()), record.file.clone());
        }
        let stem: String = deck
            .name
            .to_lowercase()
            .chars()
            .map(|character| if character.is_alphanumeric() { character } else { '-' })
            .collect();
        let stem = stem.trim_matches('-');
        let stem = if stem.is_empty() { "deck" } else { stem };
        let file = (1..)
            .map(|number| {
                if number == 1 {
                    format!("{stem}.json")
                } else {
                    format!("{stem}-{number}.json")
                }
            })
            .find(|file| !present.contains(file))
            .expect("an unused file name exists");
        (None, file)
    }

    /// What an import would do. Changes nothing.
    pub fn preview_import(&self, contents: &str) -> Result<ImportPreview, String> {
        let mut deck = parse_deck("import", contents)?;
        let (target, file_name) = self.import_target(&deck);
        deck.file.clone_from(&file_name);
        let present = HashSet::new();
        let uid = target.or_else(|| match_identity(&self.data, &deck, &present, &HashSet::new()));

        let existing = uid
            .as_ref()
            .and_then(|uid| self.library.decks.iter().find(|deck| deck.id == *uid));
        let mut preview = ImportPreview {
            deck_name: deck.name.clone(),
            file_name,
            is_update: uid.is_some(),
            total: deck.cards.len(),
            new_cards: 0,
            unchanged: 0,
            edited_unreviewed: 0,
            changed_with_history: Vec::new(),
            removed_with_history: 0,
            warnings: deck.warnings.clone(),
        };
        let mut incoming = HashSet::new();
        for card in &deck.cards {
            let key = uid.as_ref().map(|uid| format!("{uid}::{}", card.id));
            let reviews = key
                .as_ref()
                .and_then(|key| self.data.cards.get(key))
                .map(|record| record.learning.reps);
            let old = key
                .as_ref()
                .and_then(|key| existing?.cards.iter().find(|card| card.key == *key));
            if let Some(key) = &key {
                incoming.insert(key.clone());
            }
            match (reviews, old) {
                (None, _) | (Some(_), None) => preview.new_cards += 1,
                (Some(reviews), Some(old)) => {
                    let same = crate::flashcards::content_hash(&old.front, &old.back)
                        == crate::flashcards::content_hash(&card.front, &card.back);
                    if same {
                        preview.unchanged += 1;
                    } else if reviews == 0 {
                        preview.edited_unreviewed += 1;
                    } else {
                        preview.changed_with_history.push(ChangedCard {
                            key: old.key.clone(),
                            old_front: old.front.clone(),
                            old_back: old.back.clone(),
                            front: card.front.clone(),
                            back: card.back.clone(),
                            reviews,
                        });
                    }
                }
            }
        }
        if let Some(existing) = existing {
            preview.removed_with_history = existing
                .cards
                .iter()
                .filter(|card| !incoming.contains(&card.key))
                .filter(|card| {
                    self.data.cards.get(&card.key).is_some_and(|record| record.learning.reps > 0)
                })
                .count();
        }
        Ok(preview)
    }

    /// Writes the deck into the folder, keeping any file it replaces, and resets
    /// exactly the cards the player chose.
    pub fn commit_import(
        &mut self,
        folder: &Path,
        contents: &str,
        reset_keys: &[String],
        target_folder_id: Option<&str>,
        at: Moment,
    ) -> Result<Library, String> {
        if target_folder_id.is_some_and(|id| !self.data.folders.contains_key(id)) {
            return Err("That folder no longer exists. Return to the library and choose another folder.".into());
        }
        let preview = self.preview_import(contents)?;
        let path = folder.join(&preview.file_name);
        if path.exists() {
            let backups = self.store.backup_dir();
            fs::create_dir_all(&backups).map_err(|error| error.to_string())?;
            fs::copy(&path, backups.join(format!("{}.{}", at.ms, preview.file_name)))
                .map_err(|error| error.to_string())?;
        }
        fs::write(&path, contents).map_err(|error| error.to_string())?;
        let library = self.library_changed(read_library(folder), at.ms);
        if let Some(id) = target_folder_id {
            let deck = library.decks.iter().find(|deck| deck.file == preview.file_name)
                .ok_or("The imported deck could not be found.")?;
            self.organize(LibraryAction::MoveDeck { deck: deck.id.clone(), folder: Some(id.into()) }, at)?;
        }
        for key in reset_keys {
            self.reset_card(key, "chosen during import: the card tests something else now", at)?;
        }
        Ok(library)
    }

    #[cfg(test)]
    pub fn log_len(&self) -> usize {
        self.log.values().map(Vec::len).sum()
    }

    #[cfg(test)]
    pub fn history(&self, key: &str) -> &[LogEntry] {
        self.log.get(key).map_or(&[], Vec::as_slice)
    }

    #[cfg(test)]
    pub fn replayed(&self, key: &str) -> LearningState {
        let entries = self.history(key);
        replay(&self.model, entries, self.data.settings.desired_retention).unwrap()
    }
}

fn reviewed_contains(reviewed: &HashSet<String>, attempt: &Attempt) -> bool {
    reviewed.contains(&attempt.id)
}

#[cfg(test)]
#[path = "engine_tests.rs"]
mod tests;

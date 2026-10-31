//! Durable study data, kept apart from imported decks in `<AppData>/study`.
//!
//! `reviews.jsonl` is the append-only record of what actually happened and is
//! written first. `state.json` is a snapshot that can always be rebuilt from it:
//! `applied_seq` says how far it got, so a crash between the two loses nothing.
//! `sessions.json` holds each session's card snapshot and where the player was.

use std::collections::{BTreeMap, HashMap};
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use super::memory::{default_parameters, LearningState, Moment, Rating, MODEL_VERSION};
use super::session::Sessions;

pub const SCHEMA_VERSION: u32 = 1;
/// No review predates this store, so none can be reconstructed: every card
/// starts as new and no outcomes are assumed.
pub const INIT_NO_PRIOR_HISTORY: &str = "started as new; no earlier reviews existed to migrate";

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(default)]
pub struct SchedulingSettings {
    pub desired_retention: f32,
    pub new_per_day: u32,
    pub daily_minutes: f32,
}

impl Default for SchedulingSettings {
    fn default() -> Self {
        Self {
            desired_retention: 0.9,
            new_per_day: 20,
            daily_minutes: 20.0,
        }
    }
}

/// `local` is what the player chose. `utc_ms` is the instant it meant in
/// `time_zone`, which stays correct if the machine's zone changes later.
#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExamTime {
    pub utc_ms: i64,
    pub local: String,
    pub time_zone: String,
    pub date_only: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AdvancedPlanning {
    /// Share of the daily estimate expected to materialise. Observed use refines it.
    pub reliability: Option<f32>,
    pub target_recall: Option<f32>,
    pub max_new_per_day: Option<u32>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Deadline {
    pub enabled: bool,
    pub exam: ExamTime,
    /// Reserved out of the shared daily estimate. Unset decks share what remains.
    pub daily_minutes: Option<f32>,
    #[serde(default)]
    pub advanced: AdvancedPlanning,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderRecord {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub deadline: Option<Deadline>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DeckRecord {
    pub uid: String,
    pub name: String,
    pub file: String,
    #[serde(default)]
    pub file_missing: bool,
    pub created_ms: i64,
    #[serde(default)]
    pub deadline: Option<Deadline>,
    #[serde(default)]
    pub new_day: i64,
    #[serde(default)]
    pub new_count: u32,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CardRecord {
    pub deck: String,
    pub learning: LearningState,
    pub flagged_ms: Option<i64>,
    pub importance_override: Option<u8>,
    pub content_hash: String,
    /// The wording changed after it was reviewed; the player decides what to do.
    pub content_changed: bool,
    /// No longer in the deck file. Its history stays until the card returns.
    pub orphaned: bool,
    pub interruptions: u32,
    pub initialization: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct ModelRecord {
    pub parameters: Vec<f32>,
    pub model_version: String,
    pub fitted: bool,
    pub fitted_ms: Option<i64>,
    pub fit_items: usize,
    pub limited_data: bool,
    pub last_fit_error: Option<String>,
    pub previous_parameters: Option<Vec<f32>>,
}

impl Default for ModelRecord {
    fn default() -> Self {
        Self {
            parameters: default_parameters(),
            model_version: MODEL_VERSION.into(),
            fitted: false,
            fitted_ms: None,
            fit_items: 0,
            limited_data: false,
            last_fit_error: None,
            previous_parameters: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
pub struct Running {
    pub count: u32,
    pub total_s: f64,
}

impl Running {
    pub fn add(&mut self, seconds: f64) {
        self.count += 1;
        self.total_s += seconds;
    }
}

/// Durations of completed, uninterrupted reviews only.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct CostStats {
    pub new: Running,
    pub pass: Running,
    pub fail: Running,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct RatingCounts {
    pub review: [u32; 4],
    pub new: [u32; 4],
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct StudyData {
    pub schema_version: u32,
    pub applied_seq: u64,
    pub next_attempt: u64,
    pub decks: BTreeMap<String, DeckRecord>,
    pub cards: BTreeMap<String, CardRecord>,
    pub folders: BTreeMap<String, FolderRecord>,
    pub deck_folders: BTreeMap<String, Option<String>>,
    pub disabled_decks: std::collections::BTreeSet<String>,
    /// Old card key to new, when a card gained or changed its id.
    pub aliases: BTreeMap<String, String>,
    pub settings: SchedulingSettings,
    pub model: ModelRecord,
    pub costs: CostStats,
    pub ratings: RatingCounts,
    /// Measured active study seconds per study day.
    pub spent_s: BTreeMap<i64, f64>,
}

impl Default for StudyData {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            applied_seq: 0,
            next_attempt: 1,
            decks: BTreeMap::new(),
            cards: BTreeMap::new(),
            folders: BTreeMap::new(),
            deck_folders: BTreeMap::new(),
            disabled_decks: Default::default(),
            aliases: BTreeMap::new(),
            settings: SchedulingSettings::default(),
            model: ModelRecord::default(),
            costs: CostStats::default(),
            ratings: RatingCounts::default(),
            spent_s: BTreeMap::new(),
        }
    }
}

impl StudyData {
    pub fn resolve(&self, key: &str) -> String {
        let mut key = key;
        for _ in 0..self.aliases.len() {
            match self.aliases.get(key) {
                Some(next) => key = next,
                None => break,
            }
        }
        key.to_owned()
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ReviewEntry {
    pub seq: u64,
    pub id: String,
    pub card: String,
    pub at_ms: i64,
    pub utc_offset_min: i32,
    pub rating: Rating,
    pub prompt_viewed_ms: Option<i64>,
    pub revealed_ms: Option<i64>,
    pub active_ms: u64,
    pub interruptions: u32,
    pub mode: String,
}

impl ReviewEntry {
    pub fn at(&self) -> Moment {
        Moment {
            ms: self.at_ms,
            utc_offset_min: self.utc_offset_min,
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct ResetEntry {
    pub seq: u64,
    pub card: String,
    pub at_ms: i64,
    pub reason: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum LogEntry {
    Review(ReviewEntry),
    Reset(ResetEntry),
}

impl LogEntry {
    pub fn seq(&self) -> u64 {
        match self {
            LogEntry::Review(entry) => entry.seq,
            LogEntry::Reset(entry) => entry.seq,
        }
    }

    pub fn card(&self) -> &str {
        match self {
            LogEntry::Review(entry) => &entry.card,
            LogEntry::Reset(entry) => &entry.card,
        }
    }
}

pub struct Loaded {
    pub data: StudyData,
    pub sessions: Sessions,
    pub log: Vec<LogEntry>,
    /// The state file was unreadable, so everything must be replayed from the log.
    pub rebuild: bool,
}

pub struct Store {
    dir: PathBuf,
}

fn write_atomic(path: &Path, contents: &[u8]) -> io::Result<()> {
    let temp = path.with_extension("tmp");
    let mut file = File::create(&temp)?;
    file.write_all(contents)?;
    file.sync_all()?;
    fs::rename(temp, path)
}

/// Sets an unreadable file aside rather than overwriting it.
fn read_json<T: DeserializeOwned>(path: &Path) -> Result<Option<T>, ()> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(None);
    };
    serde_json::from_str(&contents).map(Some).map_err(|error| {
        eprintln!("{} is unreadable: {error}", path.display());
        let _ = fs::rename(path, path.with_extension("corrupt"));
    })
}

/// A crash mid-append leaves a partial last line, which is skipped, not fatal.
fn read_log(path: &Path) -> io::Result<Vec<LogEntry>> {
    let Ok(contents) = fs::read_to_string(path) else {
        return Ok(Vec::new());
    };
    let mut entries: Vec<LogEntry> = contents
        .lines()
        .filter(|line| !line.trim().is_empty())
        .filter_map(|line| match serde_json::from_str(line) {
            Ok(entry) => Some(entry),
            Err(error) => {
                eprintln!("skipping unreadable review log line: {error}");
                None
            }
        })
        .collect();
    entries.sort_by_key(LogEntry::seq);
    Ok(entries)
}

impl Store {
    pub fn open(dir: PathBuf) -> io::Result<(Self, Loaded)> {
        fs::create_dir_all(&dir)?;
        let store = Self { dir };
        let log = read_log(&store.log_path())?;
        let state = read_json::<StudyData>(&store.state_path());
        let rebuild = state.is_err();
        let data = state.ok().flatten().unwrap_or_default();
        let sessions = read_json::<Sessions>(&store.sessions_path())
            .ok()
            .flatten()
            .unwrap_or_default();
        Ok((
            store,
            Loaded {
                data,
                sessions,
                log,
                rebuild,
            },
        ))
    }

    fn log_path(&self) -> PathBuf {
        self.dir.join("reviews.jsonl")
    }

    fn state_path(&self) -> PathBuf {
        self.dir.join("state.json")
    }

    fn sessions_path(&self) -> PathBuf {
        self.dir.join("sessions.json")
    }

    pub fn backup_dir(&self) -> PathBuf {
        self.dir.join("deck-backups")
    }

    pub fn append(&self, entry: &LogEntry) -> Result<(), String> {
        let line = serde_json::to_string(entry).map_err(|error| error.to_string())?;
        let write = || -> io::Result<()> {
            let mut file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.log_path())?;
            // The leading newline fences off any partial line from a crash.
            file.write_all(format!("\n{line}\n").as_bytes())?;
            file.sync_data()
        };
        write().map_err(|error| error.to_string())
    }

    pub fn save_state(&self, data: &StudyData) -> Result<(), String> {
        let contents = serde_json::to_vec(data).map_err(|error| error.to_string())?;
        write_atomic(&self.state_path(), &contents).map_err(|error| error.to_string())
    }

    pub fn save_sessions(&self, sessions: &Sessions) -> Result<(), String> {
        let contents = serde_json::to_vec(sessions).map_err(|error| error.to_string())?;
        write_atomic(&self.sessions_path(), &contents).map_err(|error| error.to_string())
    }
}

/// Groups the log by current card key, following any id changes.
pub fn index_log(data: &StudyData, log: Vec<LogEntry>) -> HashMap<String, Vec<LogEntry>> {
    let mut index: HashMap<String, Vec<LogEntry>> = HashMap::new();
    for entry in log {
        index.entry(data.resolve(entry.card())).or_default().push(entry);
    }
    index
}

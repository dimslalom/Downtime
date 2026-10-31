//! The Tauri surface over `StudyEngine`. Planning and fitting run on their own
//! threads, so neither can ever hold up a keypress in the overlay.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::Duration;

use chrono::Local;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::flashcards::{FlashcardStore, Library, CHANGED_EVENT};

use super::engine::{
    CardRow, DeckSummary, FlaggedCard, ImportPreview, Measured, ModelView, StudyEngine, StudyView,
};
use super::memory::{fit_parameters, Moment, Rating};
use super::planner::plan;
use super::store::{Deadline, FolderRecord, SchedulingSettings};

const STUDY_EVENT: &str = "study-changed";
const PLAN_DEBOUNCE: Duration = Duration::from_secs(2);
const PLAN_TICK: Duration = Duration::from_secs(60);

pub struct StudyState {
    engine: Mutex<StudyEngine>,
    replan: (Mutex<bool>, Condvar),
    fitting: AtomicBool,
}

impl StudyState {
    fn engine(&self) -> std::sync::MutexGuard<'_, StudyEngine> {
        self.engine.lock().expect("study engine poisoned")
    }

    fn wake_planner(&self) {
        *self.replan.0.lock().expect("planner signal poisoned") = true;
        self.replan.1.notify_one();
    }
}

fn now() -> Moment {
    let time = Local::now();
    Moment {
        ms: time.timestamp_millis(),
        utc_offset_min: time.offset().local_minus_utc() / 60,
    }
}

fn changed<R: Runtime>(app: &AppHandle<R>) {
    if let Err(error) = app.emit(STUDY_EVENT, ()) {
        eprintln!("failed to emit {STUDY_EVENT}: {error}");
    }
}

pub fn library_changed(state: &StudyState, library: Library) -> Library {
    let library = {
        let mut engine = state.engine();
        let library = engine.library_changed(library, now().ms);
        engine.ensure_session(now());
        library
    };
    state.wake_planner();
    library
}

/// Overlay focus decides what counts as studying. It covers hiding, alt-tabbing
/// and quitting alike, and needs no change to how the overlay is shown.
pub fn overlay_focus_changed<R: Runtime>(app: &AppHandle<R>, focused: bool) {
    let state = app.state::<StudyState>();
    state.engine().set_active(focused, now());
    changed(app);
}

fn view(state: &StudyState) -> StudyView {
    state.engine().view(now())
}

#[tauri::command(async)]
pub fn load_decks(state: State<StudyState>) -> Library {
    state.engine().library().clone()
}

#[tauri::command(async)]
pub fn study_view(state: State<StudyState>) -> StudyView {
    let mut engine = state.engine();
    engine.ensure_session(now());
    engine.view(now())
}

#[tauri::command(async)]
pub fn study_flip(state: State<StudyState>, attempt_id: String) -> StudyView {
    state.engine().flip(&attempt_id, now());
    view(&state)
}

#[tauri::command(async)]
pub fn study_grade<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    attempt_id: String,
    rating: Rating,
    rest: Option<bool>,
) -> Result<StudyView, String> {
    {
        let mut engine = state.engine();
        engine.grade(&attempt_id, rating, now())?;
        if rest == Some(true) {
            engine.rest();
        }
    }
    state.wake_planner();
    changed(&app);
    Ok(view(&state))
}

#[tauri::command(async)]
pub fn study_end_rest(state: State<StudyState>) -> StudyView {
    state.engine().end_rest(now());
    view(&state)
}

#[tauri::command(async)]
pub fn study_flag<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    attempt_id: String,
) -> StudyView {
    state.engine().flag_current(&attempt_id, now());
    state.wake_planner();
    changed(&app);
    view(&state)
}

#[tauri::command(async)]
pub fn study_next_deck(state: State<StudyState>) -> StudyView {
    state.engine().next_deck(now());
    view(&state)
}

#[tauri::command(async)]
pub fn study_restart(state: State<StudyState>) -> StudyView {
    state.engine().restart(now());
    view(&state)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HomeSnapshot {
    decks: Vec<DeckSummary>,
    folders: Vec<FolderRecord>,
    flagged: Vec<FlaggedCard>,
    measured: Measured,
    model: ModelView,
    settings: SchedulingSettings,
    fitting: bool,
    folder: String,
}

#[tauri::command(async)]
pub fn home_snapshot(state: State<StudyState>) -> HomeSnapshot {
    let engine = state.engine();
    let at = now();
    HomeSnapshot {
        decks: engine.deck_summaries(at),
        folders: engine.folders(),
        flagged: engine.flagged_cards(),
        measured: engine.measured(at),
        model: engine.model_view(),
        settings: engine.settings(),
        fitting: state.fitting.load(Ordering::SeqCst),
        folder: engine.library().folder.clone(),
    }
}

#[tauri::command(async)]
pub fn deck_cards(state: State<StudyState>, deck_id: String) -> Vec<CardRow> {
    state.engine().deck_cards(&deck_id, now())
}

fn edited<R: Runtime>(app: &AppHandle<R>, state: &StudyState) {
    state.wake_planner();
    changed(app);
}

#[tauri::command(async)]
pub fn set_card_flag<R: Runtime>(app: AppHandle<R>, state: State<StudyState>, key: String, flagged: bool) {
    state.engine().set_flag(&key, flagged, now());
    edited(&app, &state);
}

#[tauri::command(async)]
pub fn set_card_importance<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    key: String,
    importance: Option<u8>,
) -> Result<(), String> {
    state.engine().set_importance(&key, importance)?;
    edited(&app, &state);
    Ok(())
}

#[tauri::command(async)]
pub fn keep_card_history<R: Runtime>(app: AppHandle<R>, state: State<StudyState>, key: String) {
    state.engine().keep_history(&key);
    edited(&app, &state);
}

#[tauri::command(async)]
pub fn reset_card_learning<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    key: String,
) -> Result<(), String> {
    state
        .engine()
        .reset_card(&key, "chosen after the card's content changed", now())?;
    edited(&app, &state);
    Ok(())
}

#[tauri::command(async)]
pub fn set_study_settings<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    settings: SchedulingSettings,
) -> Result<(), String> {
    state.engine().set_settings(settings)?;
    edited(&app, &state);
    Ok(())
}

#[tauri::command(async)]
pub fn set_deck_deadline<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    deck_id: String,
    deadline: Deadline,
) -> Result<(), String> {
    state.engine().set_deadline(&deck_id, deadline, now())?;
    edited(&app, &state);
    Ok(())
}

#[tauri::command(async)]
pub fn disable_deck_deadline<R: Runtime>(app: AppHandle<R>, state: State<StudyState>, deck_id: String) {
    state.engine().disable_deadline(&deck_id);
    edited(&app, &state);
}

#[tauri::command(async)]
pub fn preview_deck_import(state: State<StudyState>, contents: String) -> Result<ImportPreview, String> {
    state.engine().preview_import(&contents)
}

#[tauri::command(async)]
pub fn commit_deck_import<R: Runtime>(
    app: AppHandle<R>,
    state: State<StudyState>,
    store: State<FlashcardStore>,
    contents: String,
    reset_keys: Vec<String>,
    target_folder_id: Option<String>,
) -> Result<(), String> {
    let library = {
        let mut engine = state.engine();
        let library = engine.commit_import(&store.dir, &contents, &reset_keys, target_folder_id.as_deref(), now())?;
        engine.ensure_session(now());
        library
    };
    if let Err(error) = app.emit(CHANGED_EVENT, library) {
        eprintln!("failed to emit {CHANGED_EVENT}: {error}");
    }
    edited(&app, &state);
    Ok(())
}

/// Fits on its own thread. Only the snapshot and the result touch the engine,
/// so reviews carry on while it runs.
#[tauri::command(async)]
pub fn fit_memory_model<R: Runtime>(app: AppHandle<R>, state: State<StudyState>) {
    if state.fitting.swap(true, Ordering::SeqCst) {
        return;
    }
    let items = state.engine().fit_input();
    changed(&app);
    let handle = app.clone();
    thread::spawn(move || {
        let count = items.len();
        let result = fit_parameters(items);
        let state = handle.state::<StudyState>();
        state.engine().apply_fit(result, count, now());
        state.fitting.store(false, Ordering::SeqCst);
        state.wake_planner();
        changed(&handle);
    });
}

fn spawn_planner<R: Runtime>(app: AppHandle<R>) {
    thread::spawn(move || loop {
        let state = app.state::<StudyState>();
        {
            let signal = state.replan.0.lock().expect("planner signal poisoned");
            let (mut signal, _) = state
                .replan
                .1
                .wait_timeout_while(signal, PLAN_TICK, |pending| !*pending)
                .expect("planner signal poisoned");
            *signal = false;
        }
        // Lets a burst of reviews settle into one replan.
        thread::sleep(PLAN_DEBOUNCE);

        let work = {
            let mut engine = state.engine();
            if engine.needs_plan(now()) {
                engine.take_plan_request(now())
            } else {
                None
            }
        };
        if let Some((request, model)) = work {
            let output = plan(&model, &request);
            state.engine().install_plan(output, now());
            changed(&app);
        }
    });
}

pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let root: PathBuf = app.path().app_data_dir()?;
    std::fs::create_dir_all(&root)?;
    let library = Library {
        folder: root.join("flashcards").display().to_string(),
        decks: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    let engine = StudyEngine::open(root.join("study"), library)?;
    app.manage(StudyState {
        engine: Mutex::new(engine),
        replan: (Mutex::new(true), Condvar::new()),
        fitting: AtomicBool::new(false),
    });
    spawn_planner(app.clone());
    Ok(())
}

#[tauri::command(async)]
pub fn organize_library<R: Runtime>(app: AppHandle<R>, state: State<StudyState>, action: super::engine::LibraryAction) -> Result<(), String> {
    state.engine().organize(action, now())?;
    edited(&app, &state);
    Ok(())
}

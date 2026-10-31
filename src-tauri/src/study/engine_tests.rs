use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use super::*;
use crate::flashcards::{parse_deck, DeckError};
use crate::study::memory::{MemoryModel, DAY_MS};
use crate::study::scheduler::MAX_DAILY_EXPOSURES;
use crate::study::store::{AdvancedPlanning, ExamTime};

const DAY0: i64 = 20_000 * DAY_MS;

fn at(day: i64, minute: i64) -> Moment {
    Moment {
        ms: DAY0 + day * DAY_MS + 12 * 3_600_000 + minute * 60_000,
        utc_offset_min: 0,
    }
}

fn fresh_dir() -> PathBuf {
    static COUNTER: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "downtime-engine-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn library(files: &[(&str, &str)]) -> Library {
    let mut library = Library {
        folder: "folder".into(),
        decks: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };
    for (stem, contents) in files {
        match parse_deck(stem, contents) {
            Ok(deck) => library.decks.push(deck),
            Err(message) => library.errors.push(DeckError {
                file: format!("{stem}.json"),
                message,
            }),
        }
    }
    library
}

fn deck_json(name: &str, count: usize) -> String {
    let cards: Vec<String> = (0..count)
        .map(|index| format!(r#"{{"id": "c{index}", "front": "Q{index}", "back": "A{index}"}}"#))
        .collect();
    format!(r#"{{"deck_name": "{name}", "cards": [{}]}}"#, cards.join(","))
}

fn open_with(dir: &PathBuf, files: &[(&str, &str)]) -> StudyEngine {
    let mut engine = StudyEngine::open(dir.clone(), library(files)).unwrap();
    engine.ensure_session(at(0, 0));
    engine
}

fn engine(count: usize) -> (StudyEngine, PathBuf) {
    let dir = fresh_dir();
    let engine = open_with(&dir, &[("bio", &deck_json("Bio", count))]);
    (engine, dir)
}

fn current(engine: &StudyEngine, when: Moment) -> CardView {
    engine.view(when).session.unwrap().card.expect("a card is showing")
}

fn reveal_and_grade(engine: &mut StudyEngine, rating: Rating, when: Moment) -> CardView {
    let card = current(engine, when);
    engine.flip(&card.attempt_id, when);
    engine.grade(&card.attempt_id, rating, when).unwrap();
    card
}

fn exam(when: Moment) -> ExamTime {
    ExamTime {
        utc_ms: when.ms,
        local: "2024-10-20T09:00".into(),
        time_zone: "Australia/Brisbane".into(),
        date_only: false,
    }
}

fn deadline(when: Moment) -> Deadline {
    Deadline {
        enabled: true,
        exam: exam(when),
        daily_minutes: None,
        advanced: AdvancedPlanning::default(),
    }
}

#[test]
fn migration_starts_every_card_new_and_invents_no_reviews() {
    let (engine, _dir) = engine(5);
    assert_eq!(engine.log_len(), 0);
    for record in engine.data.cards.values() {
        assert_eq!(record.learning, LearningState::default());
        assert_eq!(record.initialization, INIT_NO_PRIOR_HISTORY);
    }
}

#[test]
fn a_grade_records_ratings_and_the_memory_state_replays_exactly() {
    let (mut engine, _dir) = engine(2);
    let first = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let record = &engine.data.cards[&first.key];
    assert_eq!(record.learning.phase, Phase::Review);
    assert_eq!(engine.history(&first.key).len(), 1);
    assert_eq!(record.learning, engine.replayed(&first.key));
}

#[test]
fn grading_requires_a_visible_answer() {
    let (mut engine, _dir) = engine(2);
    let card = current(&engine, at(0, 0));
    engine.grade(&card.attempt_id, Rating::Good, at(0, 0)).unwrap();
    assert_eq!(engine.log_len(), 0);
    assert_eq!(current(&engine, at(0, 0)).attempt_id, card.attempt_id);
}

#[test]
fn duplicate_grades_are_recorded_once() {
    let (mut engine, _dir) = engine(3);
    let when = at(0, 0);
    let card = current(&engine, when);
    engine.flip(&card.attempt_id, when);
    for _ in 0..5 {
        engine.grade(&card.attempt_id, Rating::Good, when).unwrap();
    }
    assert_eq!(engine.log_len(), 1);
    assert_eq!(engine.data.cards[&card.key].learning.reps, 1);
    assert_eq!(engine.view(when).session.unwrap().passed, 1);
    assert_ne!(current(&engine, when).attempt_id, card.attempt_id);
}

#[test]
fn a_retry_after_restart_is_still_a_duplicate() {
    let (mut engine, dir) = engine(3);
    let when = at(0, 0);
    let card = reveal_and_grade(&mut engine, Rating::Good, when);
    drop(engine);

    let mut reopened = open_with(&dir, &[("bio", &deck_json("Bio", 3))]);
    reopened.grade(&card.attempt_id, Rating::Again, at(0, 1)).unwrap();
    assert_eq!(reopened.log_len(), 1);
    assert_eq!(reopened.data.cards[&card.key].learning.reps, 1);
}

#[test]
fn hiding_revealing_and_quitting_never_grade() {
    let (mut engine, dir) = engine(3);
    let when = at(0, 0);
    engine.set_active(true, when);
    let card = current(&engine, when);
    engine.flip(&card.attempt_id, at(0, 1));
    engine.set_active(false, at(0, 2));
    engine.set_active(true, at(0, 30));
    engine.set_active(false, at(0, 31));
    drop(engine);

    let reopened = open_with(&dir, &[("bio", &deck_json("Bio", 3))]);
    assert_eq!(reopened.log_len(), 0);
    assert_eq!(reopened.data.cards[&card.key].learning, LearningState::default());
    assert!(reopened.data.cards[&card.key].interruptions >= 2);
}

#[test]
fn the_same_card_and_flip_state_survive_a_restart() {
    let (mut engine, dir) = engine(3);
    let when = at(0, 0);
    engine.set_active(true, when);
    let card = current(&engine, when);
    engine.flip(&card.attempt_id, when);
    drop(engine);

    let reopened = open_with(&dir, &[("bio", &deck_json("Bio", 3))]);
    let view = reopened.view(at(0, 5)).session.unwrap();
    assert!(view.flipped);
    assert_eq!(view.card.unwrap().attempt_id, card.attempt_id);
}

#[test]
fn hidden_time_is_excluded_from_the_review_duration() {
    let (mut engine, _dir) = engine(2);
    let start = at(0, 0);
    engine.set_active(true, start);
    let card = current(&engine, start);
    let soon = Moment { ms: start.ms + 4_000, ..start };
    engine.flip(&card.attempt_id, soon);
    engine.set_active(false, soon);
    let back = at(0, 45);
    engine.set_active(true, back);
    let done = Moment { ms: back.ms + 3_000, ..back };
    engine.grade(&card.attempt_id, Rating::Good, done).unwrap();

    let LogEntry::Review(review) = &engine.history(&card.key)[0] else {
        panic!("expected a review");
    };
    assert_eq!(review.active_ms, 7_000);
    assert_eq!(review.interruptions, 1);
    assert_eq!(review.prompt_viewed_ms, Some(start.ms));
    assert_eq!(review.revealed_ms, Some(soon.ms));
}

#[test]
fn only_uninterrupted_reviews_teach_the_cost_model() {
    let (mut engine, _dir) = engine(3);
    let start = at(0, 0);
    engine.set_active(true, start);

    let clean = current(&engine, start);
    let later = Moment { ms: start.ms + 9_000, ..start };
    engine.flip(&clean.attempt_id, later);
    engine.grade(&clean.attempt_id, Rating::Good, later).unwrap();
    assert_eq!(engine.data.costs.new.count, 1);

    let broken = current(&engine, later);
    engine.flip(&broken.attempt_id, later);
    engine.set_active(false, later);
    engine.set_active(true, at(0, 5));
    engine.grade(&broken.attempt_id, Rating::Good, at(0, 5)).unwrap();
    assert_eq!(engine.data.costs.new.count, 1);
}

#[test]
fn a_crash_after_logging_loses_nothing_and_does_not_repeat_the_card() {
    let (mut engine, dir) = engine(3);
    let when = at(0, 0);
    let card = current(&engine, when);
    engine.flip(&card.attempt_id, when);
    let stale_state = fs::read(dir.join("state.json")).unwrap();
    let stale_sessions = fs::read(dir.join("sessions.json")).unwrap();
    engine.grade(&card.attempt_id, Rating::Good, when).unwrap();
    drop(engine);
    // As if the process died after the log append but before anything else.
    fs::write(dir.join("state.json"), stale_state).unwrap();
    fs::write(dir.join("sessions.json"), stale_sessions).unwrap();

    let reopened = open_with(&dir, &[("bio", &deck_json("Bio", 3))]);
    assert_eq!(reopened.data.cards[&card.key].learning.reps, 1);
    assert_ne!(
        reopened.view(when).session.unwrap().card.map(|view| view.key),
        Some(card.key)
    );
}

#[test]
fn a_corrupt_state_file_rebuilds_history_from_the_log() {
    let (mut engine, dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Easy, at(0, 0));
    let expected = engine.data.cards[&card.key].learning.clone();
    drop(engine);
    fs::write(dir.join("state.json"), "{corrupt").unwrap();

    let reopened = open_with(&dir, &[("bio", &deck_json("Bio", 2))]);
    assert_eq!(reopened.data.cards[&card.key].learning, expected);
}

#[test]
fn a_partial_last_log_line_is_skipped() {
    let (mut engine, dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    drop(engine);
    let log = dir.join("reviews.jsonl");
    let mut contents = fs::read_to_string(&log).unwrap();
    contents.push_str("{\"kind\":\"review\",\"seq\":9");
    fs::write(&log, contents).unwrap();

    let mut reopened = open_with(&dir, &[("bio", &deck_json("Bio", 2))]);
    assert_eq!(reopened.log_len(), 1);
    reveal_and_grade(&mut reopened, Rating::Good, at(0, 1));
    assert_eq!(reopened.log_len(), 2);
    drop(reopened);
    let again = open_with(&dir, &[("bio", &deck_json("Bio", 2))]);
    assert_eq!(again.log_len(), 2);
    assert_eq!(again.data.cards[&card.key].learning.reps, 1);
}

#[test]
fn a_changed_deck_does_not_disturb_a_session_in_progress() {
    let (mut engine, _dir) = engine(2);
    let when = at(0, 0);
    let card = current(&engine, when);
    engine.flip(&card.attempt_id, when);

    let edited = deck_json("Bio", 2).replace("\"Q0\"", "\"Rewritten\"");
    engine.library_changed(library(&[("bio", &edited)]), when.ms);
    let view = engine.view(when).session.unwrap();
    assert!(view.flipped);
    assert_eq!(view.card.unwrap().front, card.front);
}

#[test]
fn reimport_preserves_history_and_the_next_session_uses_new_content() {
    let (mut engine, _dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));

    let edited = deck_json("Bio", 3).replace("\"A0\"", "\"Better answer\"");
    engine.library_changed(library(&[("bio", &edited)]), at(0, 1).ms);
    assert_eq!(engine.data.cards[&card.key].learning.reps, 1);
    assert!(engine.data.cards[&card.key].content_changed);
    assert_eq!(engine.history(&card.key).len(), 1);
    assert_eq!(engine.data.cards.len(), 3);
}

#[test]
fn resetting_a_changed_card_keeps_raw_history_but_clears_memory() {
    let (mut engine, _dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    engine.reset_card(&card.key, "different question", at(1, 0)).unwrap();

    let record = &engine.data.cards[&card.key];
    assert_eq!(record.learning, LearningState::default());
    assert!(!record.content_changed);
    assert_eq!(engine.history(&card.key).len(), 2);
    assert_eq!(engine.replayed(&card.key), LearningState::default());
}

#[test]
fn keeping_history_only_clears_the_decision() {
    let (mut engine, _dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let edited = deck_json("Bio", 2).replace("\"A0\"", "\"Clearer\"");
    engine.library_changed(library(&[("bio", &edited)]), at(0, 1).ms);
    engine.keep_history(&card.key);
    assert!(!engine.data.cards[&card.key].content_changed);
    assert_eq!(engine.data.cards[&card.key].learning.reps, 1);
}

#[test]
fn import_preview_counts_changes_without_applying_them() {
    let (mut engine, dir) = engine(3);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let edited = deck_json("Bio", 4).replace("\"A0\"", "\"New\"");
    let preview = engine.preview_import(&edited).unwrap();
    assert!(preview.is_update);
    assert_eq!(preview.file_name, "bio.json");
    assert_eq!(preview.new_cards, 1);
    assert_eq!(preview.changed_with_history.len(), 1);
    assert_eq!(preview.changed_with_history[0].key, card.key);
    assert_eq!(preview.changed_with_history[0].reviews, 1);
    assert_eq!(engine.data.cards.len(), 3);
    assert!(!engine.data.cards[&card.key].content_changed);
    drop(dir);
}

#[test]
fn import_preview_warns_before_cards_with_history_are_removed() {
    let (mut engine, _dir) = engine(2);
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let shorter = r#"{"deck_name": "Bio", "cards": [{"id": "other", "front": "X", "back": "Y"}]}"#;
    assert_eq!(engine.preview_import(shorter).unwrap().removed_with_history, 1);
}

#[test]
fn committing_an_import_backs_up_the_old_file_and_applies_chosen_resets() {
    let folder = fresh_dir();
    let original = deck_json("Bio", 2);
    fs::write(folder.join("bio.json"), &original).unwrap();
    let mut engine = StudyEngine::open(fresh_dir(), crate::flashcards::read_library(&folder)).unwrap();
    engine.ensure_session(at(0, 0));
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));

    let edited = original.replace("\"Q0\"", "\"Different question\"");
    let reset = vec![card.key.clone()];
    engine.commit_import(&folder, &edited, &reset, None, at(0, 1)).unwrap();

    assert_eq!(fs::read_to_string(folder.join("bio.json")).unwrap(), edited);
    assert_eq!(engine.data.cards[&card.key].learning.reps, 0);
    assert_eq!(engine.history(&card.key).len(), 2);
    assert_eq!(fs::read_dir(engine.store.backup_dir()).unwrap().count(), 1);
}

#[test]
fn an_invalid_import_changes_nothing() {
    let folder = fresh_dir();
    let (mut engine, _dir) = engine(2);
    let bad = r#"{"deck_name": "Bio", "cards": [{"front": "Q", "back": "A", "importance": 9}]}"#;
    assert!(engine.commit_import(&folder, bad, &[], None, at(0, 0)).is_err());
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
}

#[test]
fn importing_into_a_folder_persists_membership_and_inherits_policy() {
    let folder = fresh_dir();
    let state = fresh_dir();
    let mut engine = StudyEngine::open(state.clone(), crate::flashcards::read_library(&folder)).unwrap();
    engine.organize(LibraryAction::CreateFolder { name: "Biology".into() }, at(0, 0)).unwrap();
    let id = engine.folders()[0].id.clone();
    engine.organize(LibraryAction::FolderDeadline { id: id.clone(), deadline: deadline(at(10, 0)) }, at(0, 0)).unwrap();
    let library = engine.commit_import(&folder, &deck_json("Bio", 2), &[], Some(&id), at(0, 0)).unwrap();
    let uid = library.decks[0].id.clone();
    assert_eq!(engine.data.deck_folders[&uid], Some(id.clone()));
    assert_eq!(engine.mode(&uid, at(0, 0)), Mode::Deadline);
    assert_eq!(engine.log_len(), 0);
    drop(engine);
    let mut engine = StudyEngine::open(state, crate::flashcards::read_library(&folder)).unwrap();
    assert_eq!(engine.data.deck_folders[&uid], Some(id.clone()));
    // An ordinary reimport preserves its existing folder and learning policy.
    engine.commit_import(&folder, &deck_json("Bio", 3), &[], None, at(0, 1)).unwrap();
    assert_eq!(engine.data.deck_folders[&uid], Some(id));
}

#[test]
fn importing_into_a_missing_folder_does_not_write_a_deck() {
    let folder = fresh_dir();
    let (mut engine, _) = engine(1);
    assert!(engine.commit_import(&folder, &deck_json("New", 2), &[], Some("missing"), at(0, 0)).is_err());
    assert_eq!(fs::read_dir(&folder).unwrap().count(), 0);
    assert_eq!(engine.log_len(), 0);
}

#[test]
fn flagging_moves_on_without_grading_and_can_be_undone() {
    let (mut engine, _dir) = engine(3);
    let when = at(0, 0);
    let card = current(&engine, when);
    engine.flag_current(&card.attempt_id, when);

    assert_eq!(engine.log_len(), 0);
    assert_eq!(engine.data.cards[&card.key].learning, LearningState::default());
    assert_ne!(current(&engine, when).key, card.key);
    let flagged = engine.flagged_cards();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].front, card.front);

    engine.set_flag(&card.key, false, when);
    assert!(engine.flagged_cards().is_empty());
}

#[test]
fn flagged_cards_come_after_everything_else_and_stay_in_the_coverage_count() {
    let (mut engine, _dir) = engine(2);
    let when = at(0, 0);
    let first = current(&engine, when);
    engine.flag_current(&first.attempt_id, when);
    let other = reveal_and_grade(&mut engine, Rating::Good, when);
    assert_ne!(other.key, first.key);
    assert!(engine.view(when).session.unwrap().card.is_none());

    let summary = &engine.deck_summaries(when)[0];
    assert_eq!(summary.card_count, 2);
    assert_eq!(summary.flagged, 1);
    assert_eq!(summary.unseen, 1);
}

#[test]
fn pressing_flag_on_a_flagged_card_unflags_it_and_stays() {
    let (mut engine, _dir) = engine(1);
    let when = at(0, 0);
    let card = current(&engine, when);
    engine.flag_current(&card.attempt_id, when);
    engine.restart(when);
    let again = current(&engine, when);
    assert!(again.flagged);
    engine.flag_current(&again.attempt_id, when);
    let after = current(&engine, when);
    assert_eq!(after.attempt_id, again.attempt_id);
    assert!(!after.flagged);
}

#[test]
fn again_brings_a_card_back_after_its_step_not_at_once() {
    let (mut engine, _dir) = engine(2);
    let first = reveal_and_grade(&mut engine, Rating::Again, at(0, 0));
    assert_ne!(current(&engine, at(0, 0)).key, first.key);
    reveal_and_grade(&mut engine, Rating::Good, at(0, 1));
    assert!(engine.view(at(0, 1)).session.unwrap().card.is_none());

    let reopened_at = at(0, 11);
    engine.set_active(true, reopened_at);
    assert_eq!(current(&engine, reopened_at).key, first.key);
    assert!(current(&engine, reopened_at).retry);
}

#[test]
fn retry_uses_saved_last_grade_and_clears_after_success() {
    let (mut engine, dir) = engine(1);
    assert!(!current(&engine, at(0, 0)).retry);
    reveal_and_grade(&mut engine, Rating::Again, at(0, 0));
    drop(engine);

    let mut engine = open_with(&dir, &[("bio", &deck_json("Bio", 1))]);
    engine.set_active(true, at(0, 11));
    assert!(current(&engine, at(0, 11)).retry);
    reveal_and_grade(&mut engine, Rating::Hard, at(0, 11));
    engine.set_active(true, at(1, 0));
    engine.restart(at(1, 0));
    assert!(!current(&engine, at(1, 0)).retry);
}

#[test]
fn a_stubborn_card_cannot_monopolise_a_day() {
    let (mut engine, _dir) = engine(1);
    let mut minute = 0;
    for _ in 0..MAX_DAILY_EXPOSURES {
        engine.set_active(true, at(0, minute));
        reveal_and_grade(&mut engine, Rating::Again, at(0, minute));
        engine.set_active(false, at(0, minute));
        minute += 11;
    }
    engine.set_active(true, at(0, minute));
    assert!(engine.view(at(0, minute)).session.unwrap().card.is_none());
}

#[test]
fn the_new_card_limit_is_per_deck_per_day() {
    let (mut engine, _dir) = engine(4);
    let mut settings = engine.settings();
    settings.new_per_day = 2;
    engine.set_settings(settings).unwrap();
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    assert!(engine.view(at(0, 0)).session.unwrap().card.is_none());

    engine.set_active(true, at(1, 0));
    assert!(engine.view(at(1, 0)).session.unwrap().card.is_some());
}

#[test]
fn finishing_offers_extra_practice_on_request_only() {
    let (mut engine, _dir) = engine(1);
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    assert!(engine.view(at(1, 0)).session.unwrap().card.is_none());
    engine.restart(at(1, 0));
    let view = engine.view(at(1, 0)).session.unwrap();
    assert!(view.extra);
    assert!(view.card.is_some());
}

#[test]
fn settings_outside_the_supported_range_are_rejected() {
    let (mut engine, _dir) = engine(1);
    let mut settings = engine.settings();
    settings.desired_retention = 0.99;
    assert!(engine.set_settings(settings).is_err());
}

#[test]
fn changing_desired_retention_moves_due_dates_but_not_memory() {
    let (mut engine, _dir) = engine(1);
    let card = reveal_and_grade(&mut engine, Rating::Easy, at(0, 0));
    let before = engine.data.cards[&card.key].learning.clone();
    let mut settings = engine.settings();
    settings.desired_retention = 0.75;
    engine.set_settings(settings).unwrap();
    let after = &engine.data.cards[&card.key].learning;
    assert_eq!(after.memory, before.memory);
    assert!(after.due_day > before.due_day);
}

#[test]
fn deadlines_are_stored_apart_from_decks_and_must_be_in_the_future() {
    let (mut engine, _dir) = engine(2);
    assert!(engine.set_deadline("bio", deadline(at(-1, 0)), at(0, 0)).is_err());
    engine.set_deadline("bio", deadline(at(7, 0)), at(0, 0)).unwrap();
    assert_eq!(engine.mode("bio", at(0, 0)), Mode::Deadline);
    let summaries = engine.deck_summaries(at(0, 0));
    assert_eq!(summaries[0].deadline.as_ref().unwrap().exam.time_zone, "Australia/Brisbane");
}

#[test]
fn deadlines_survive_a_restart_and_a_deck_edit() {
    let (mut engine, dir) = engine(2);
    engine.set_deadline("bio", deadline(at(7, 0)), at(0, 0)).unwrap();
    drop(engine);
    let reopened = open_with(&dir, &[("bio", &deck_json("Bio", 5))]);
    assert_eq!(reopened.mode("bio", at(0, 0)), Mode::Deadline);
}

#[test]
fn the_plan_can_move_a_review_ahead_of_its_fsrs_due_date() {
    let (mut engine, _dir) = engine(1);
    let card = reveal_and_grade(&mut engine, Rating::Easy, at(0, 0));
    let due = engine.data.cards[&card.key].learning.due_day.unwrap();
    let when = at(6, 0);
    assert!(due > when.study_day());
    engine.set_active(true, when);
    assert!(engine.view(when).session.unwrap().card.is_none());

    engine.set_deadline("bio", deadline(at(7, 0)), when).unwrap();
    engine.replan_now(when);
    let view = engine.view(when).session.unwrap();
    assert_eq!(view.mode, Mode::Deadline);
    assert_eq!(view.card.map(|card| card.key), Some(card.key.clone()));
    // Planning alone changed nothing about the card.
    assert_eq!(engine.data.cards[&card.key].learning.due_day, Some(due));
    assert_eq!(engine.history(&card.key).len(), 1);
}

#[test]
fn moving_the_deadline_replans() {
    let (mut engine, _dir) = engine(30);
    let when = at(0, 0);
    let mut settings = engine.settings();
    settings.daily_minutes = 3.0;
    engine.set_settings(settings).unwrap();
    engine.set_deadline("bio", deadline(at(30, 0)), when).unwrap();
    assert!(engine.needs_plan(when));
    engine.replan_now(when);
    assert!(!engine.needs_plan(when));
    let far = engine.deck_summaries(when)[0].plan.clone().unwrap();

    engine.set_deadline("bio", deadline(at(2, 0)), when).unwrap();
    assert!(engine.needs_plan(when));
    engine.replan_now(when);
    let near = engine.deck_summaries(when)[0].plan.clone().unwrap();
    assert!(near.estimate.expected_introduced < far.estimate.expected_introduced, "{near:?} {far:?}");
}

#[test]
fn a_plan_goes_stale_on_a_new_day_so_missed_days_are_replanned() {
    let (mut engine, _dir) = engine(3);
    engine.set_deadline("bio", deadline(at(9, 0)), at(0, 0)).unwrap();
    engine.replan_now(at(0, 0));
    assert!(!engine.needs_plan(at(0, 5)));
    assert!(engine.needs_plan(at(3, 0)));
}

#[test]
fn an_expired_deadline_stops_planning_keeps_the_date_and_resumes_ordinary_review() {
    let (mut engine, _dir) = engine(2);
    engine.set_deadline("bio", deadline(at(1, 0)), at(0, 0)).unwrap();
    let later = at(2, 0);
    assert_eq!(engine.mode("bio", later), Mode::Expired);
    assert!(!engine.needs_plan(later));
    let summary = &engine.deck_summaries(later)[0];
    assert_eq!(summary.deadline.as_ref().unwrap().exam.utc_ms, at(1, 0).ms);
    assert_eq!(summary.card_count, 2);
    assert!(summary.plan.is_none());
    engine.set_active(true, later);
    assert!(engine.view(later).session.unwrap().card.is_some());
}

#[test]
fn disabling_a_deadline_keeps_history_and_due_dates() {
    let (mut engine, _dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    engine.set_deadline("bio", deadline(at(5, 0)), at(0, 1)).unwrap();
    engine.replan_now(at(0, 1));
    let before = engine.data.cards[&card.key].learning.clone();

    engine.disable_deadline("bio");
    assert_eq!(engine.mode("bio", at(0, 2)), Mode::Ordinary);
    assert_eq!(engine.data.cards[&card.key].learning, before);
    assert_eq!(engine.history(&card.key).len(), 1);
    assert!(!engine.data.decks["bio"].deadline.as_ref().unwrap().enabled);
}

#[test]
fn deadline_reviews_run_through_the_same_memory_model() {
    let (mut engine, _dir) = engine(1);
    engine.set_deadline("bio", deadline(at(5, 0)), at(0, 0)).unwrap();
    engine.replan_now(at(0, 0));
    engine.set_active(true, at(0, 0));
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    assert_eq!(engine.data.cards[&card.key].learning, engine.replayed(&card.key));
    let LogEntry::Review(review) = &engine.history(&card.key)[0] else {
        panic!("expected a review");
    };
    assert_eq!(review.mode, "deadline");
}

#[test]
fn importance_edits_never_touch_parameters_or_memory() {
    let (mut engine, _dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let before = (engine.data.model.parameters.clone(), engine.data.cards[&card.key].learning.clone());
    engine.set_importance(&card.key, Some(5)).unwrap();
    engine.set_deadline("bio", deadline(at(5, 0)), at(0, 1)).unwrap();
    engine.replan_now(at(0, 1));
    assert_eq!(engine.data.model.parameters, before.0);
    assert_eq!(engine.data.cards[&card.key].learning, before.1);
    assert!(engine.set_importance(&card.key, Some(6)).is_err());
}

#[test]
fn a_shared_budget_is_not_promised_to_each_deck() {
    let dir = fresh_dir();
    let (one, two) = (deck_json("One", 80), deck_json("Two", 80));
    let mut engine = open_with(&dir, &[("one", &one), ("two", &two)]);
    let mut settings = engine.settings();
    settings.daily_minutes = 4.0;
    engine.set_settings(settings).unwrap();
    engine.set_deadline("one", deadline(at(10, 0)), at(0, 0)).unwrap();
    engine.set_deadline("two", deadline(at(12, 0)), at(0, 0)).unwrap();
    engine.replan_now(at(0, 0));

    let request = engine.plan_request(at(0, 0)).unwrap();
    assert_eq!(request.groups.len(), 1);
    let plans = engine.plan.as_ref().unwrap();
    let minutes: f32 = plans.decks.iter().map(|deck| deck.estimate.today_minutes).sum();
    assert!(minutes <= 4.0, "{minutes}");
}

#[test]
fn per_deck_reservations_are_scaled_to_the_shared_estimate() {
    let dir = fresh_dir();
    let (one, two) = (deck_json("One", 20), deck_json("Two", 20));
    let mut engine = open_with(&dir, &[("one", &one), ("two", &two)]);
    let mut settings = engine.settings();
    settings.daily_minutes = 10.0;
    engine.set_settings(settings).unwrap();
    for uid in ["one", "two"] {
        let mut chosen = deadline(at(10, 0));
        chosen.daily_minutes = Some(10.0);
        engine.set_deadline(uid, chosen, at(0, 0)).unwrap();
    }
    let request = engine.plan_request(at(0, 0)).unwrap();
    let total: f32 = request.groups.iter().map(|group| group.today_s).sum();
    assert!((total - 600.0).abs() < 1.0, "{total}");
}

#[test]
fn time_already_spent_today_comes_off_the_budget() {
    let (mut engine, _dir) = engine(40);
    let mut settings = engine.settings();
    settings.daily_minutes = 2.0;
    engine.set_settings(settings).unwrap();
    engine.set_deadline("bio", deadline(at(9, 0)), at(0, 0)).unwrap();
    let before = engine.plan_request(at(0, 0)).unwrap().groups[0].today_s;
    engine.data.spent_s.insert(at(0, 0).study_day(), 90.0);
    let after = engine.plan_request(at(0, 0)).unwrap().groups[0].today_s;
    assert!(after < before / 2.0, "{before} {after}");
}

#[test]
fn a_plan_marks_coverage_that_exceeds_the_time_available() {
    let (mut engine, _dir) = engine(300);
    let mut settings = engine.settings();
    settings.daily_minutes = 2.0;
    engine.set_settings(settings).unwrap();
    engine.set_deadline("bio", deadline(at(3, 0)), at(0, 0)).unwrap();
    engine.replan_now(at(0, 0));
    let plan = engine.deck_summaries(at(0, 0))[0].plan.clone().unwrap();
    assert_eq!(plan.fit, Fit::Over);
    assert!(plan.expected_unseen_at_exam > 100.0, "{plan:?}");
    assert!(plan.minutes_to_cover_unseen > plan.minutes_available);
}

#[test]
fn a_comfortable_plan_reports_that_it_fits() {
    let (mut engine, _dir) = engine(10);
    engine.set_deadline("bio", deadline(at(14, 0)), at(0, 0)).unwrap();
    engine.replan_now(at(0, 0));
    let plan = engine.deck_summaries(at(0, 0))[0].plan.clone().unwrap();
    assert_ne!(plan.fit, Fit::Over, "{plan:?}");
    assert!(plan.estimate.exam_recall_weighted > 0.5, "{plan:?}");
}

#[test]
fn observed_use_refines_how_much_of_the_budget_is_trusted() {
    let (mut engine, _dir) = engine(2);
    let today = at(20, 0).study_day();
    assert_eq!(engine.reliability(None, today), DEFAULT_RELIABILITY);
    assert_eq!(engine.reliability(Some(0.4), today), 0.4);
    for day in (today - 14)..today {
        engine.data.spent_s.insert(day, 0.0);
    }
    assert!(engine.reliability(None, today) < DEFAULT_RELIABILITY);
}

#[test]
fn fitting_with_too_little_data_keeps_the_parameters_and_says_so() {
    let (mut engine, _dir) = engine(2);
    reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let before = engine.data.model.parameters.clone();
    engine.fit_blocking(at(0, 1));
    assert_eq!(engine.data.model.parameters, before);
    assert!(!engine.data.model.fitted);
    assert!(engine.data.model.last_fit_error.is_some());
}

#[test]
fn a_failed_fit_retains_the_previous_parameters() {
    let (mut engine, _dir) = engine(2);
    let before = engine.data.model.parameters.clone();
    engine.apply_fit(Err("optimizer broke".into()), 50, at(0, 0));
    assert_eq!(engine.data.model.parameters, before);
    assert!(engine.data.model.last_fit_error.as_ref().unwrap().contains("optimizer broke"));
}

#[test]
fn a_successful_fit_replays_memory_from_the_raw_log() {
    let (mut engine, _dir) = engine(1);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    let mut parameters = engine.data.model.parameters.clone();
    parameters[2] = 4.0;
    engine.apply_fit(
        Ok(FitOutcome::Fitted { parameters: parameters.clone(), limited_data: true }),
        12,
        at(0, 1),
    );
    let record = &engine.data.cards[&card.key];
    assert!((record.learning.memory.unwrap().stability - 4.0).abs() < 1e-4);
    assert!(engine.data.model.fitted);
    assert!(engine.data.model.limited_data);
    assert!(engine.data.model.previous_parameters.is_some());
    assert_eq!(engine.model.parameters, parameters);
}

#[test]
fn fit_input_targets_delayed_reviews_and_respects_resets() {
    let (mut engine, _dir) = engine(1);
    let when = at(0, 0);
    let card = reveal_and_grade(&mut engine, Rating::Good, when);
    engine.set_active(true, at(2, 0));
    reveal_and_grade(&mut engine, Rating::Good, at(2, 0));
    assert_eq!(engine.fit_input().len(), 1);
    engine.reset_card(&card.key, "changed", at(2, 1)).unwrap();
    assert_eq!(engine.fit_input().len(), 1);
    assert_eq!(MemoryModel::new(&engine.data.model.parameters).is_ok(), true);
}

#[test]
fn opening_before_the_folder_is_read_orphans_nothing() {
    let (mut engine, dir) = engine(2);
    let card = reveal_and_grade(&mut engine, Rating::Good, at(0, 0));
    drop(engine);
    let reopened = StudyEngine::open(dir, library(&[])).unwrap();
    assert!(!reopened.data.cards[&card.key].orphaned);
    assert!(!reopened.data.decks["bio"].file_missing);
}


#[test]
fn course_folders_migrate_once_and_keep_history_and_ids() {
    let dir = fresh_dir();
    let files = [("comp-a", deck_json("COMP3506 - Graphs", 2)), ("comp-b", deck_json("COMP3506 - Trees", 2))];
    let borrowed: Vec<(&str, &str)> = files.iter().map(|(f, j)| (*f, j.as_str())).collect();
    let mut engine = open_with(&dir, &borrowed);
    assert_eq!(engine.folders().len(), 1);
    assert_eq!(engine.folders()[0].name, "COMP3506");
    let before = current(&engine, at(0, 0));
    engine.organize(LibraryAction::MoveDeck { deck: "comp-a".into(), folder: None }, at(0, 0)).unwrap();
    engine.library_changed(library(&borrowed), at(0, 0).ms);
    assert!(engine.data.deck_folders["comp-a"].is_none());
    assert_eq!(current(&engine, at(0, 0)).key, before.key);
    drop(engine);
    let engine = open_with(&dir, &borrowed);
    assert!(engine.data.deck_folders["comp-a"].is_none());
    assert_eq!(engine.data.deck_folders["comp-b"].as_deref(), Some("comp3506"));
}

#[test]
fn overlay_folder_switch_preserves_deck_choices_and_flipped_attempts() {
    let dir = fresh_dir();
    let a = deck_json("COMP3506 - Graphs", 2);
    let b = deck_json("COMP3506 - Trees", 2);
    let mut engine = open_with(&dir, &[("a", &a), ("b", &b)]);
    let before = current(&engine, at(0, 0));
    engine.flip(&before.attempt_id, at(0, 0));
    engine.organize(LibraryAction::SelectDeck { deck: "b".into(), enabled: false }, at(0, 0)).unwrap();
    engine.organize(LibraryAction::SelectFolder { id: "comp3506".into(), enabled: false }, at(0, 0)).unwrap();
    assert_eq!(engine.view(at(0, 0)).deck_count, 0);
    assert!(engine.view(at(0, 0)).session.is_none());
    engine.next_deck(at(0, 0)); // No modulo by zero when every deck is disabled.
    engine.organize(LibraryAction::SelectFolder { id: "comp3506".into(), enabled: true }, at(0, 0)).unwrap();
    assert_eq!(engine.view(at(0, 0)).deck_count, 1);
    assert_eq!(current(&engine, at(0, 0)).attempt_id, before.attempt_id);
    assert!(engine.view(at(0, 0)).session.unwrap().flipped);
    assert!(!engine.overlay_enabled("b"));
    assert_eq!(engine.log_len(), 0);
    drop(engine);
    let engine = open_with(&dir, &[("a", &a), ("b", &b)]);
    assert!(!engine.overlay_enabled("b"));
}

#[test]
fn folder_deadlines_share_reservations_and_allow_deck_overrides() {
    let dir = fresh_dir();
    let a = deck_json("COMP3506 - Graphs", 2);
    let b = deck_json("COMP3506 - Trees", 2);
    let mut engine = open_with(&dir, &[("a", &a), ("b", &b)]);
    let mut shared = deadline(at(10, 0));
    shared.daily_minutes = Some(10.0);
    engine.organize(LibraryAction::FolderDeadline { id: "comp3506".into(), deadline: shared.clone() }, at(0, 0)).unwrap();
    let request = engine.plan_request(at(0, 0)).unwrap();
    assert_eq!(request.decks.len(), 2);
    assert_eq!(request.groups.len(), 1);
    assert_eq!(request.decks[0].group, request.decks[1].group);
    assert!((request.groups[0].today_s - 600.0).abs() < 0.01);
    assert!(engine.deck_summaries(at(0, 0)).iter().all(|d| d.deadline_inherited));
    engine.set_deadline("a", deadline(at(20, 0)), at(0, 0)).unwrap();
    assert_eq!(engine.effective_deadline("a").unwrap().exam.utc_ms, at(20, 0).ms);
    assert_eq!(engine.effective_deadline("b").unwrap().exam.utc_ms, at(10, 0).ms);
    engine.disable_deadline("b");
    assert_eq!(engine.mode("b", at(0, 0)), Mode::Ordinary);
    engine.organize(LibraryAction::InheritDeadline { deck: "b".into() }, at(0, 0)).unwrap();
    assert_eq!(engine.mode("b", at(0, 0)), Mode::Deadline);
    engine.organize(LibraryAction::SelectDeck { deck: "a".into(), enabled: false }, at(0, 0)).unwrap();
    let request = engine.plan_request(at(0, 0)).unwrap();
    assert_eq!(request.decks.len(), 1);
    assert_eq!(request.decks[0].uid, "b");
    assert_eq!(engine.mode("b", at(11, 0)), Mode::Expired);
    assert_eq!(engine.log_len(), 0);
}

#[test]
fn old_state_defaults_to_enabled_unfiled_library() {
    let data: StudyData = serde_json::from_str(r#"{"schema_version":1}"#).unwrap();
    assert!(data.folders.is_empty());
    assert!(data.deck_folders.is_empty());
    assert!(data.disabled_decks.is_empty());
}

#[test]
fn moving_and_removing_folders_changes_policy_without_changing_cards() {
    let (mut engine, _) = engine(2);
    let before = current(&engine, at(0, 0));
    engine.organize(LibraryAction::CreateFolder { name: "Biology".into() }, at(0, 0)).unwrap();
    let id = engine.folders()[0].id.clone();
    engine.organize(LibraryAction::FolderDeadline { id: id.clone(), deadline: deadline(at(10, 0)) }, at(0, 0)).unwrap();
    engine.organize(LibraryAction::MoveDeck { deck: "bio".into(), folder: Some(id.clone()) }, at(0, 0)).unwrap();
    assert_eq!(engine.mode("bio", at(0, 0)), Mode::Deadline);
    engine.organize(LibraryAction::DeleteFolder { id }, at(0, 0)).unwrap();
    assert_eq!(engine.mode("bio", at(0, 0)), Mode::Ordinary);
    assert_eq!(current(&engine, at(0, 0)).attempt_id, before.attempt_id);
    assert_eq!(engine.log_len(), 0);
}

#[test]
fn a_card_behind_the_rest_pause_has_not_started() {
    let (mut engine, _dir) = engine(3);
    let start = at(0, 0);
    engine.set_active(true, start);
    reveal_and_grade(&mut engine, Rating::Good, start);
    engine.rest();
    let card = current(&engine, start);
    let attempt = engine.sessions.by_deck["bio"].current.clone().unwrap();
    assert_eq!(attempt.prompt_viewed_ms, None);

    let hidden = Moment { ms: start.ms + 3_000, ..start };
    engine.set_active(false, hidden);
    let back = at(0, 5);
    engine.set_active(true, back);
    let done = Moment { ms: back.ms + 12_000, ..back };
    engine.flip(&card.attempt_id, done);
    engine.grade(&card.attempt_id, Rating::Good, done).unwrap();

    let LogEntry::Review(review) = &engine.history(&card.key)[0] else {
        panic!("expected a review");
    };
    assert_eq!(review.active_ms, 12_000);
    assert_eq!(review.interruptions, 0);
    assert_eq!(review.prompt_viewed_ms, Some(back.ms));
    assert_eq!(engine.data.costs.new.count, 1);
}

#[test]
fn asking_for_one_more_starts_the_card_behind_the_pause() {
    let (mut engine, _dir) = engine(3);
    let start = at(0, 0);
    engine.set_active(true, start);
    reveal_and_grade(&mut engine, Rating::Good, start);
    engine.rest();
    let shown = Moment { ms: start.ms + 2_000, ..start };
    engine.end_rest(shown);
    let attempt = engine.sessions.by_deck["bio"].current.clone().unwrap();
    assert_eq!(attempt.prompt_viewed_ms, Some(shown.ms));
    assert_eq!(attempt.resumed_ms, Some(shown.ms));
}

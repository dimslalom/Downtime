use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use notify::{RecommendedWatcher, RecursiveMode};
use notify_debouncer_mini::{new_debouncer, DebounceEventResult, Debouncer};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use crate::study::commands::{library_changed, StudyState};

pub const CHANGED_EVENT: &str = "decks-changed";
const SAMPLE_DECK: &str = include_str!("../sample-deck.json");
pub const LEGACY_ID_PREFIX: &str = "legacy:";
pub const NEUTRAL_IMPORTANCE: u8 = 3;
const MAX_ID_LENGTH: usize = 128;
const MAX_REPORTED_ERRORS: usize = 12;

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum CardType {
    Definition,
    Relationship,
    Exception,
    #[serde(rename = "Method Choice")]
    MethodChoice,
}

const CARD_TYPES: [(&str, CardType); 4] = [
    ("Definition", CardType::Definition),
    ("Relationship", CardType::Relationship),
    ("Exception", CardType::Exception),
    ("Method Choice", CardType::MethodChoice),
];

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Card {
    /// Stable within its deck: the supplied `id`, or derived from the front.
    pub id: String,
    /// `<deck identity>::<id>`, assigned once the deck is matched to its identity.
    pub key: String,
    pub explicit_id: bool,
    pub card_type: Option<CardType>,
    pub front: String,
    pub back: String,
    pub source: Option<String>,
    pub importance: Option<u8>,
    pub importance_reason: Option<String>,
    pub assessment_evidence: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Deck {
    /// The file stem until matched, then the persistent deck identity.
    pub id: String,
    pub file: String,
    pub name: String,
    pub cards: Vec<Card>,
    #[serde(skip)]
    pub warnings: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DeckError {
    pub file: String,
    pub message: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct Library {
    pub folder: String,
    pub decks: Vec<Deck>,
    pub errors: Vec<DeckError>,
    pub warnings: Vec<DeckError>,
}

/// FNV-1a: the id must not change between Rust releases, unlike `DefaultHasher`.
pub fn stable_hash(text: &str) -> String {
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    });
    format!("{hash:016x}")
}

/// Whitespace and capitalisation changes should not count as a different card.
pub fn normalized(text: &str) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}

pub fn legacy_id(front: &str) -> String {
    format!("{LEGACY_ID_PREFIX}{}", stable_hash(&normalized(front)))
}

pub fn content_hash(front: &str, back: &str) -> String {
    stable_hash(&format!("{}\0{}", normalized(front), normalized(back)))
}

fn optional_text(field: &str, value: Option<&Value>, errors: &mut Vec<String>) -> Option<String> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::String(text)) if text.trim().is_empty() => None,
        Some(Value::String(text)) => Some(text.trim().to_owned()),
        Some(other) => {
            errors.push(format!("{field} must be text or null, got {other}"));
            None
        }
    }
}

fn required_text(field: &str, value: Option<&Value>, errors: &mut Vec<String>) -> String {
    match value {
        Some(Value::String(text)) if !text.trim().is_empty() => text.clone(),
        Some(Value::String(_)) => {
            errors.push(format!("{field} is empty"));
            String::new()
        }
        None | Some(Value::Null) => {
            errors.push(format!("{field} is missing"));
            String::new()
        }
        Some(other) => {
            errors.push(format!("{field} must be text, got {other}"));
            String::new()
        }
    }
}

fn parse_id(value: Option<&Value>, errors: &mut Vec<String>) -> Option<String> {
    let id = optional_text("id", value, errors)?;
    if id.starts_with(LEGACY_ID_PREFIX) {
        errors.push(format!("id must not start with \"{LEGACY_ID_PREFIX}\", got \"{id}\""));
    } else if id.chars().count() > MAX_ID_LENGTH {
        errors.push(format!("id is longer than {MAX_ID_LENGTH} characters"));
    }
    Some(id)
}

fn parse_card_type(value: Option<&Value>, errors: &mut Vec<String>) -> Option<CardType> {
    let text = optional_text("card_type", value, errors)?;
    let known = CARD_TYPES
        .iter()
        .find(|(name, _)| name.eq_ignore_ascii_case(&text))
        .map(|&(_, card_type)| card_type);
    if known.is_none() {
        errors.push(format!(
            "card_type \"{text}\" is not supported. Use Definition, Relationship, Exception or Method Choice"
        ));
    }
    known
}

fn parse_importance(value: Option<&Value>, errors: &mut Vec<String>) -> Option<u8> {
    match value {
        None | Some(Value::Null) => None,
        Some(Value::Number(number)) => match number.as_u64() {
            Some(importance @ 1..=5) => Some(importance as u8),
            _ => {
                errors.push(format!("importance must be a whole number from 1 to 5, got {number}"));
                None
            }
        },
        Some(other) => {
            errors.push(format!("importance must be a whole number from 1 to 5, got {other}"));
            None
        }
    }
}

fn parse_card(value: &Value, errors: &mut Vec<String>) -> Card {
    let Some(fields) = value.as_object() else {
        errors.push(format!("must be an object with front and back, got {value}"));
        return empty_card();
    };
    let explicit = parse_id(fields.get("id"), errors);
    let front = required_text("front", fields.get("front"), errors);
    let back = required_text("back", fields.get("back"), errors);
    Card {
        explicit_id: explicit.is_some(),
        id: explicit.unwrap_or_else(|| legacy_id(&front)),
        key: String::new(),
        card_type: parse_card_type(fields.get("card_type"), errors),
        source: optional_text("source", fields.get("source"), errors),
        importance: parse_importance(fields.get("importance"), errors),
        importance_reason: optional_text("importance_reason", fields.get("importance_reason"), errors),
        assessment_evidence: optional_text(
            "assessment_evidence",
            fields.get("assessment_evidence"),
            errors,
        ),
        front,
        back,
    }
}

fn empty_card() -> Card {
    Card {
        id: String::new(),
        key: String::new(),
        explicit_id: false,
        card_type: None,
        front: String::new(),
        back: String::new(),
        source: None,
        importance: None,
        importance_reason: None,
        assessment_evidence: None,
    }
}

/// Exact repeats of a card without an id collapse into the first. Cards sharing
/// a front but not a back stay separate, and appending one leaves the first's id
/// alone, so its history stays attached.
fn resolve_legacy_duplicates(cards: Vec<Card>, warnings: &mut Vec<String>) -> Vec<Card> {
    let mut seen: HashMap<String, String> = HashMap::new();
    let mut kept: Vec<Card> = Vec::with_capacity(cards.len());
    for (index, mut card) in cards.into_iter().enumerate() {
        if card.explicit_id {
            kept.push(card);
            continue;
        }
        let content = content_hash(&card.front, &card.back);
        match seen.get(&card.id) {
            None => {
                seen.insert(card.id.clone(), content);
                kept.push(card);
            }
            Some(first) if *first == content => warnings.push(format!(
                "Card {} repeats an earlier card and was skipped.",
                index + 1
            )),
            Some(_) => {
                warnings.push(format!(
                    "Card {} has the same front as an earlier card but a different back. Both are kept. Consider rewording one so they test different things.",
                    index + 1
                ));
                card.id = format!("{LEGACY_ID_PREFIX}{content}");
                if kept.iter().any(|other| other.id == card.id) {
                    continue;
                }
                kept.push(card);
            }
        }
    }
    kept
}

fn join_errors(errors: Vec<String>) -> String {
    let hidden = errors.len().saturating_sub(MAX_REPORTED_ERRORS);
    let mut lines: Vec<String> = errors.into_iter().take(MAX_REPORTED_ERRORS).collect();
    if hidden > 0 {
        lines.push(format!("...and {hidden} more."));
    }
    lines.join("\n")
}

pub fn parse_deck(id: &str, contents: &str) -> Result<Deck, String> {
    // PowerShell and Notepad can both write a BOM, which serde_json rejects.
    let contents = contents.strip_prefix('\u{feff}').unwrap_or(contents);
    let file: Value = serde_json::from_str(contents)
        .map_err(|error| format!("This is not valid JSON: {error}"))?;

    let name = match file.get("deck_name") {
        Some(Value::String(name)) if !name.trim().is_empty() => name.trim().to_owned(),
        _ => return Err("deck_name is missing. Add \"deck_name\": \"Your deck\" at the top.".into()),
    };
    let Some(values) = file.get("cards").and_then(Value::as_array) else {
        return Err("cards is missing. Add a \"cards\" list of {\"front\", \"back\"} objects.".into());
    };
    if values.is_empty() {
        return Err(
            "This deck has no cards, so there is nothing to study. An AI returning zero cards means the material had no suitable targets."
                .into(),
        );
    }

    let mut errors = Vec::new();
    let mut cards = Vec::with_capacity(values.len());
    let mut ids: HashMap<String, usize> = HashMap::new();
    for (index, value) in values.iter().enumerate() {
        let mut card_errors = Vec::new();
        let card = parse_card(value, &mut card_errors);
        let label = if card.explicit_id {
            format!("Card {} (id \"{}\")", index + 1, card.id)
        } else {
            format!("Card {}", index + 1)
        };
        errors.extend(card_errors.into_iter().map(|error| format!("{label}: {error}.")));

        if card.explicit_id {
            if let Some(first) = ids.insert(card.id.clone(), index + 1) {
                errors.push(format!(
                    "{label}: this id is already used by card {first}. Every id must be unique in the deck."
                ));
            }
        }
        cards.push(card);
    }
    if !errors.is_empty() {
        return Err(join_errors(errors));
    }

    let mut warnings = Vec::new();
    let cards = resolve_legacy_duplicates(cards, &mut warnings);
    Ok(Deck {
        id: id.to_owned(),
        file: format!("{id}.json"),
        name,
        cards,
        warnings,
    })
}

fn is_deck_file(path: &Path) -> bool {
    path.extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
}

pub fn read_library(dir: &Path) -> Library {
    let mut library = Library {
        folder: dir.display().to_string(),
        decks: Vec::new(),
        errors: Vec::new(),
        warnings: Vec::new(),
    };

    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(error) => {
            library.errors.push(DeckError {
                file: library.folder.clone(),
                message: error.to_string(),
            });
            return library;
        }
    };

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.is_file() && is_deck_file(path))
        .collect();
    paths.sort();

    for path in paths {
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let id = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        let deck = fs::read_to_string(&path)
            .map_err(|error| error.to_string())
            .and_then(|contents| parse_deck(&id, &contents));
        match deck {
            Ok(mut deck) => {
                deck.file = file;
                library.decks.push(deck);
            }
            Err(message) => library.errors.push(DeckError { file, message }),
        }
    }
    library
}

pub struct FlashcardStore {
    pub dir: PathBuf,
    _watcher: Debouncer<RecommendedWatcher>,
}

#[tauri::command]
pub fn open_flashcards_folder(store: State<FlashcardStore>) -> Result<(), String> {
    std::process::Command::new("explorer")
        .arg(&store.dir)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// Creates `<AppData>/flashcards` on first run and starts watching it.
pub fn setup<R: Runtime>(app: &AppHandle<R>) -> Result<(), Box<dyn std::error::Error>> {
    let dir = app.path().app_data_dir()?.join("flashcards");
    if !dir.exists() {
        fs::create_dir_all(&dir)?;
        fs::write(dir.join("getting-started.json"), SAMPLE_DECK)?;
    }

    let emitter = app.clone();
    let watched = dir.clone();
    // Debounced so one save, or a burst of files from an AI tool, reloads once.
    let mut watcher = new_debouncer(
        Duration::from_millis(250),
        move |result: DebounceEventResult| match result {
            Ok(_) => {
                let state = emitter.state::<StudyState>();
                let library = library_changed(&state, read_library(&watched));
                if let Err(error) = emitter.emit(CHANGED_EVENT, library) {
                    eprintln!("failed to emit {CHANGED_EVENT}: {error}");
                }
            }
            Err(error) => eprintln!("flashcard watcher error: {error}"),
        },
    )?;
    watcher.watcher().watch(&dir, RecursiveMode::NonRecursive)?;

    let state = app.state::<StudyState>();
    library_changed(&state, read_library(&dir));

    app.manage(FlashcardStore {
        dir,
        _watcher: watcher,
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_documented_schema() {
        let deck = parse_deck(
            "bio",
            r#"{"deck_name": "Name", "cards": [{"front": "Q", "back": "A"}]}"#,
        )
        .unwrap();
        assert_eq!(deck.name, "Name");
        assert_eq!(deck.cards.len(), 1);
        assert_eq!(deck.cards[0].front, "Q");
        assert_eq!(deck.cards[0].back, "A");
        assert!(!deck.cards[0].explicit_id);
        assert_eq!(deck.cards[0].importance, None);
    }

    #[test]
    fn parses_the_rich_schema() {
        let deck = parse_deck(
            "algo",
            r#"{"deck_name": "Algorithms", "cards": [{
                "id": "shortest-paths-001",
                "card_type": "Exception",
                "front": "Why can negative edge weights invalidate Dijkstra's greedy finalization?",
                "back": "A later path can reduce a distance already treated as final.",
                "source": "Lecture 5, slide 18",
                "importance": 4,
                "importance_reason": "Supports selecting and justifying shortest-path algorithms.",
                "assessment_evidence": null
            }]}"#,
        )
        .unwrap();
        let card = &deck.cards[0];
        assert_eq!(card.id, "shortest-paths-001");
        assert!(card.explicit_id);
        assert_eq!(card.card_type, Some(CardType::Exception));
        assert_eq!(card.source.as_deref(), Some("Lecture 5, slide 18"));
        assert_eq!(card.importance, Some(4));
        assert_eq!(card.assessment_evidence, None);
    }

    #[test]
    fn accepts_method_choice_and_null_metadata() {
        let deck = parse_deck(
            "n",
            r#"{"deck_name": "N", "cards": [{"front": "Q", "back": "A",
                "card_type": "method choice", "importance": null, "source": null}]}"#,
        )
        .unwrap();
        assert_eq!(deck.cards[0].card_type, Some(CardType::MethodChoice));
    }

    #[test]
    fn accepts_a_byte_order_mark_and_extra_fields() {
        let contents =
            "\u{feff}{\"deck_name\": \"N\", \"source\": \"ai\", \"cards\": [{\"front\": \"Q\", \"back\": \"A\", \"tag\": 1}]}";
        assert!(parse_deck("n", contents).is_ok());
    }

    #[test]
    fn rejects_malformed_and_empty_decks() {
        assert!(parse_deck("n", "{not json").is_err());
        assert!(parse_deck("n", r#"{"deck_name": "N"}"#).is_err());
        assert!(parse_deck("n", r#"{"deck_name": "N", "cards": []}"#).is_err());
        assert!(parse_deck("n", r#"{"cards": [{"front": "Q", "back": "A"}]}"#).is_err());
    }

    #[test]
    fn names_the_card_and_the_fix_for_invalid_metadata() {
        for (card, expected) in [
            (r#"{"front": "Q", "back": "A", "importance": 7}"#, "1 to 5, got 7"),
            (r#"{"front": "Q", "back": "A", "importance": 2.5}"#, "1 to 5, got 2.5"),
            (r#"{"front": "Q", "back": "A", "importance": "high"}"#, "1 to 5"),
            (r#"{"front": "Q", "back": "A", "card_type": "Fact"}"#, "Method Choice"),
            (r#"{"front": "Q", "back": "A", "source": 12}"#, "source must be text"),
            (r#"{"front": "", "back": "A"}"#, "front is empty"),
            (r#"{"back": "A"}"#, "front is missing"),
            (r#"{"front": "Q", "back": "A", "id": "legacy:abc"}"#, "must not start"),
        ] {
            let contents = format!(r#"{{"deck_name": "N", "cards": [{card}]}}"#);
            let error = parse_deck("n", &contents).unwrap_err();
            assert!(error.contains(expected), "{card}: {error}");
            assert!(error.starts_with("Card 1"), "{error}");
        }
    }

    #[test]
    fn reports_every_invalid_card_at_once() {
        let error = parse_deck(
            "n",
            r#"{"deck_name": "N", "cards": [
                {"front": "Q", "back": "A", "importance": 9},
                {"front": "Q2", "back": "A"},
                {"front": "Q3", "back": ""}
            ]}"#,
        )
        .unwrap_err();
        assert!(error.contains("Card 1"));
        assert!(error.contains("Card 3"));
        assert!(!error.contains("Card 2"));
    }

    #[test]
    fn rejects_duplicate_ids() {
        let error = parse_deck(
            "n",
            r#"{"deck_name": "N", "cards": [
                {"id": "same", "front": "Q1", "back": "A"},
                {"id": "same", "front": "Q2", "back": "B"}
            ]}"#,
        )
        .unwrap_err();
        assert!(error.contains("already used by card 1"), "{error}");
    }

    #[test]
    fn derives_stable_legacy_ids_and_handles_duplicates() {
        let deck = parse_deck(
            "n",
            r#"{"deck_name": "N", "cards": [
                {"front": "Same  Question", "back": "A"},
                {"front": "same question", "back": "a"},
                {"front": "Same Question", "back": "Different"}
            ]}"#,
        )
        .unwrap();
        assert_eq!(deck.cards.len(), 2);
        assert_eq!(deck.cards[0].id, legacy_id("same question"));
        assert_ne!(deck.cards[0].id, deck.cards[1].id);
        assert_eq!(deck.warnings.len(), 2);
    }

    #[test]
    fn sample_deck_is_valid() {
        assert!(parse_deck("sample", SAMPLE_DECK).is_ok());
    }

    #[test]
    fn reads_only_json_files_and_reports_bad_ones() {
        let dir = std::env::temp_dir().join(format!("downtime-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("a.json"), SAMPLE_DECK).unwrap();
        fs::write(dir.join("b.JSON"), "nope").unwrap();
        fs::write(dir.join("notes.txt"), "ignored").unwrap();

        let library = read_library(&dir);
        fs::remove_dir_all(&dir).unwrap();

        assert_eq!(library.decks.len(), 1);
        assert_eq!(library.decks[0].id, "a");
        assert_eq!(library.errors.len(), 1);
        assert_eq!(library.errors[0].file, "b.JSON");
    }
}

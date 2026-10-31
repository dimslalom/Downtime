//! Ties deck files to persistent identities, so history survives a reimport.
//!
//! A deck keeps its identity when its file does. If the file was renamed, it is
//! recognised by deck name, then by sharing at least half its card ids with a
//! deck whose file disappeared. Cards are `<deck uid>::<card id>`. Cards missing
//! from a file become orphans, keeping their history until they return.

use std::collections::{HashMap, HashSet};

use crate::flashcards::{content_hash, legacy_id, Deck, DeckError, Library};

use super::store::{CardRecord, DeckRecord, StudyData, INIT_NO_PRIOR_HISTORY};

fn slug(text: &str) -> String {
    let slug = text
        .to_lowercase()
        .chars()
        .map(|character| if character.is_alphanumeric() { character } else { '-' })
        .collect::<String>();
    let slug = slug.split('-').filter(|part| !part.is_empty()).collect::<Vec<_>>().join("-");
    if slug.is_empty() {
        "deck".into()
    } else {
        slug
    }
}

fn new_uid(data: &StudyData, deck: &Deck) -> String {
    let base = slug(deck.file.trim_end_matches(".json").trim_end_matches(".JSON"));
    if !data.decks.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|suffix| format!("{base}-{suffix}"))
        .find(|uid| !data.decks.contains_key(uid))
        .expect("an unused suffix exists")
}

fn shared_cards(data: &StudyData, uid: &str, deck: &Deck) -> usize {
    deck.cards
        .iter()
        .filter(|card| data.cards.contains_key(&format!("{uid}::{}", card.id)))
        .count()
}

/// Finds the identity a deck file belongs to, without changing anything.
pub fn match_identity(
    data: &StudyData,
    deck: &Deck,
    present_files: &HashSet<String>,
    claimed: &HashSet<String>,
) -> Option<String> {
    let available = |record: &&DeckRecord| !claimed.contains(&record.uid);
    let vacated = |record: &&DeckRecord| {
        !claimed.contains(&record.uid) && !present_files.contains(&record.file.to_lowercase())
    };

    if let Some(record) = data
        .decks
        .values()
        .filter(available)
        .find(|record| record.file.eq_ignore_ascii_case(&deck.file))
    {
        return Some(record.uid.clone());
    }
    if let Some(record) = data
        .decks
        .values()
        .filter(vacated)
        .find(|record| record.name.eq_ignore_ascii_case(&deck.name))
    {
        return Some(record.uid.clone());
    }
    data.decks
        .values()
        .filter(vacated)
        .map(|record| (shared_cards(data, &record.uid, deck), record))
        .filter(|&(shared, _)| shared > 0 && shared * 2 >= deck.cards.len())
        .max_by_key(|&(shared, _)| shared)
        .map(|(_, record)| record.uid.clone())
}

fn previous_key(data: &StudyData, uid: &str, card_front: &str, hash: &str, key: &str, current: &HashSet<String>) -> Option<String> {
    let legacy = format!("{uid}::{}", legacy_id(card_front));
    if legacy != key && data.cards.contains_key(&legacy) && !current.contains(&legacy) {
        return Some(legacy);
    }
    data.cards
        .iter()
        .find(|(other, record)| {
            record.deck == uid
                && record.content_hash == hash
                && *other != key
                && !current.contains(*other)
        })
        .map(|(other, _)| other.clone())
}

fn upsert_cards(data: &mut StudyData, uid: &str, deck: &mut Deck) -> Vec<(String, String)> {
    let mut moved = Vec::new();
    let current: HashSet<String> = deck
        .cards
        .iter()
        .map(|card| format!("{uid}::{}", card.id))
        .collect();

    for card in &mut deck.cards {
        let key = format!("{uid}::{}", card.id);
        let hash = content_hash(&card.front, &card.back);
        card.key.clone_from(&key);

        if !data.cards.contains_key(&key) {
            match previous_key(data, uid, &card.front, &hash, &key, &current) {
                Some(previous) => {
                    let record = data.cards.remove(&previous).expect("record exists");
                    data.cards.insert(key.clone(), record);
                    data.aliases.remove(&key);
                    data.aliases.insert(previous.clone(), key.clone());
                    moved.push((previous, key.clone()));
                }
                None => {
                    data.cards.insert(
                        key.clone(),
                        CardRecord {
                            deck: uid.to_owned(),
                            content_hash: hash.clone(),
                            initialization: INIT_NO_PRIOR_HISTORY.into(),
                            ..CardRecord::default()
                        },
                    );
                }
            }
        }

        let record = data.cards.get_mut(&key).expect("record exists");
        record.orphaned = false;
        if record.content_hash != hash {
            record.content_changed = record.learning.reps > 0;
            record.content_hash = hash;
        }
    }

    for (key, record) in &mut data.cards {
        if record.deck == uid && !current.contains(key) {
            record.orphaned = true;
        }
    }
    moved
}

/// Aligns the study data with what is on disk and gives every deck and card its
/// persistent id. Returns keys that moved, so the log index can follow.
pub fn reconcile(data: &mut StudyData, library: &mut Library, now_ms: i64) -> Vec<(String, String)> {
    let mut present_files: HashSet<String> =
        library.decks.iter().map(|deck| deck.file.to_lowercase()).collect();
    // A file that fails to parse is still there: its cards are not orphans.
    present_files.extend(library.errors.iter().map(|error| error.file.to_lowercase()));

    let mut claimed = HashSet::new();
    let mut moved = Vec::new();
    let mut names: HashMap<String, String> = HashMap::new();
    let mut warnings: Vec<DeckError> = Vec::new();

    for deck in &mut library.decks {
        let uid = match_identity(data, deck, &present_files, &claimed).unwrap_or_else(|| {
            let uid = new_uid(data, deck);
            data.decks.insert(
                uid.clone(),
                DeckRecord {
                    uid: uid.clone(),
                    name: deck.name.clone(),
                    file: deck.file.clone(),
                    file_missing: false,
                    created_ms: now_ms,
                    deadline: None,
                    new_day: 0,
                    new_count: 0,
                },
            );
            uid
        });
        claimed.insert(uid.clone());

        let record = data.decks.get_mut(&uid).expect("identity exists");
        record.name.clone_from(&deck.name);
        record.file.clone_from(&deck.file);
        record.file_missing = false;
        deck.id = uid.clone();
        moved.extend(upsert_cards(data, &uid, deck));

        warnings.extend(deck.warnings.iter().map(|message| DeckError {
            file: deck.file.clone(),
            message: message.clone(),
        }));
        if let Some(other) = names.insert(deck.name.to_lowercase(), deck.file.clone()) {
            warnings.push(DeckError {
                file: deck.file.clone(),
                message: format!(
                    "{other} uses the same deck name. They are tracked as separate decks."
                ),
            });
        }
    }

    let missing: Vec<String> = data
        .decks
        .values()
        .filter(|record| {
            !claimed.contains(&record.uid) && !present_files.contains(&record.file.to_lowercase())
        })
        .map(|record| record.uid.clone())
        .collect();
    for uid in missing {
        if let Some(record) = data.decks.get_mut(&uid) {
            record.file_missing = true;
        }
        for record in data.cards.values_mut().filter(|record| record.deck == uid) {
            record.orphaned = true;
        }
    }

    library.warnings = warnings;
    moved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flashcards::parse_deck;
    use crate::study::memory::{MemoryModel, Moment, Rating};

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

    fn sync(data: &mut StudyData, files: &[(&str, &str)]) -> Library {
        let mut library = library(files);
        reconcile(data, &mut library, 1);
        library
    }

    fn review(data: &mut StudyData, key: &str) {
        let at = Moment { ms: 1_700_000_000_000, utc_offset_min: 0 };
        let model = MemoryModel::default();
        let record = data.cards.get_mut(key).unwrap();
        record.learning = model.apply(&record.learning, Rating::Good, at, 0.9).unwrap();
    }

    const LEGACY: &str = r#"{"deck_name": "Bio", "cards": [{"front": "Q1", "back": "A1"}, {"front": "Q2", "back": "A2"}]}"#;

    #[test]
    fn identifies_decks_by_file_then_keeps_history_when_the_name_changes() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let key = first.decks[0].cards[0].key.clone();
        assert!(key.starts_with("bio::legacy:"));
        review(&mut data, &key);

        let renamed = LEGACY.replace("\"Bio\"", "\"Biology 101\"");
        let second = sync(&mut data, &[("bio", &renamed)]);
        assert_eq!(second.decks[0].id, "bio");
        assert_eq!(second.decks[0].cards[0].key, key);
        assert_eq!(data.cards[&key].learning.reps, 1);
        assert_eq!(data.decks["bio"].name, "Biology 101");
    }

    #[test]
    fn a_renamed_file_keeps_its_identity_by_deck_name() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let key = first.decks[0].cards[0].key.clone();
        review(&mut data, &key);

        let second = sync(&mut data, &[("biology-final", LEGACY)]);
        assert_eq!(second.decks[0].id, "bio");
        assert_eq!(data.cards[&key].learning.reps, 1);
        assert!(!data.cards[&key].orphaned);
    }

    #[test]
    fn a_renamed_file_and_deck_match_on_shared_cards() {
        let mut data = StudyData::default();
        sync(&mut data, &[("bio", LEGACY)]);
        let renamed = LEGACY.replace("\"Bio\"", "\"Cells\"");
        let second = sync(&mut data, &[("cells", &renamed)]);
        assert_eq!(second.decks[0].id, "bio");
    }

    #[test]
    fn two_files_with_one_name_stay_separate_and_warn() {
        let mut data = StudyData::default();
        let both = sync(&mut data, &[("a", LEGACY), ("b", LEGACY)]);
        assert_ne!(both.decks[0].id, both.decks[1].id);
        assert_ne!(both.decks[0].cards[0].key, both.decks[1].cards[0].key);
        assert_eq!(both.warnings.len(), 1);
    }

    #[test]
    fn removed_cards_keep_history_and_return_intact() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let key = first.decks[0].cards[0].key.clone();
        review(&mut data, &key);

        sync(
            &mut data,
            &[("bio", r#"{"deck_name": "Bio", "cards": [{"front": "Q2", "back": "A2"}]}"#)],
        );
        assert!(data.cards[&key].orphaned);
        assert_eq!(data.cards[&key].learning.reps, 1);

        sync(&mut data, &[("bio", LEGACY)]);
        assert!(!data.cards[&key].orphaned);
        assert_eq!(data.cards[&key].learning.reps, 1);
    }

    #[test]
    fn a_file_that_fails_to_parse_does_not_orphan_its_cards() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let key = first.decks[0].cards[0].key.clone();
        sync(&mut data, &[("bio", "{broken")]);
        assert!(!data.cards[&key].orphaned);
        assert!(!data.decks["bio"].file_missing);
    }

    #[test]
    fn a_deleted_file_keeps_its_identity_and_cards() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let key = first.decks[0].cards[0].key.clone();
        sync(&mut data, &[]);
        assert!(data.decks["bio"].file_missing);
        assert!(data.cards[&key].orphaned);
    }

    #[test]
    fn stable_ids_survive_edits_and_flag_the_change() {
        let mut data = StudyData::default();
        let v1 = r#"{"deck_name": "Bio", "cards": [{"id": "c1", "front": "Q", "back": "A"}]}"#;
        sync(&mut data, &[("bio", v1)]);
        review(&mut data, "bio::c1");

        let cosmetic = r#"{"deck_name": "Bio", "cards": [{"id": "c1", "front": "q ", "back": "a"}]}"#;
        sync(&mut data, &[("bio", cosmetic)]);
        assert!(!data.cards["bio::c1"].content_changed);

        let edited = r#"{"deck_name": "Bio", "cards": [{"id": "c1", "front": "Other", "back": "A"}]}"#;
        sync(&mut data, &[("bio", edited)]);
        let record = &data.cards["bio::c1"];
        assert!(record.content_changed);
        assert_eq!(record.learning.reps, 1);
    }

    #[test]
    fn editing_an_unreviewed_card_needs_no_decision() {
        let mut data = StudyData::default();
        let v1 = r#"{"deck_name": "Bio", "cards": [{"id": "c1", "front": "Q", "back": "A"}]}"#;
        sync(&mut data, &[("bio", v1)]);
        sync(&mut data, &[("bio", &v1.replace("\"A\"", "\"B\""))]);
        assert!(!data.cards["bio::c1"].content_changed);
    }

    #[test]
    fn adding_ids_to_a_legacy_deck_moves_history_onto_them() {
        let mut data = StudyData::default();
        let first = sync(&mut data, &[("bio", LEGACY)]);
        let legacy_key = first.decks[0].cards[0].key.clone();
        review(&mut data, &legacy_key);

        let upgraded = r#"{"deck_name": "Bio", "cards": [
            {"id": "one", "front": "Q1", "back": "A1 extended"},
            {"id": "two", "front": "Q2", "back": "A2"}
        ]}"#;
        sync(&mut data, &[("bio", upgraded)]);
        assert!(!data.cards.contains_key(&legacy_key));
        assert_eq!(data.cards["bio::one"].learning.reps, 1);
        assert!(data.cards["bio::one"].content_changed);
        assert_eq!(data.resolve(&legacy_key), "bio::one");
    }

    #[test]
    fn a_changed_id_with_the_same_content_keeps_its_history() {
        let mut data = StudyData::default();
        let v1 = r#"{"deck_name": "Bio", "cards": [{"id": "old", "front": "Q", "back": "A"}]}"#;
        sync(&mut data, &[("bio", v1)]);
        review(&mut data, "bio::old");
        sync(&mut data, &[("bio", &v1.replace("\"old\"", "\"new\""))]);
        assert_eq!(data.cards["bio::new"].learning.reps, 1);
        assert_eq!(data.resolve("bio::old"), "bio::new");
    }
}

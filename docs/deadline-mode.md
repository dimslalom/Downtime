# Memory model and deadline policy

Downtime keeps four things apart.

| Layer | Holds | Lives in |
| --- | --- | --- |
| Content | What to learn | `flashcards/*.json` |
| Memory | What FSRS infers from real reviews | `study/reviews.jsonl`, `study/state.json` |
| Scheduling policy | When to study, including deadlines and budgets | `study/state.json` |
| Plan | Today's list and the estimates | Memory only, rebuilt on a background thread |

Importance and deadlines reach only the plan. They never touch memory state or fitted parameters. A test checks this.

## Memory

Memory uses the official `fsrs` crate, pinned to 6.6.2 (FSRS-6). Each card keeps a stability, a difficulty and its last review. Recall predictions come from the library's forgetting curve.

- **Raw history.** Every grade goes to the append-only `reviews.jsonl` first, with its timestamp, UTC offset, rating and timing. `state.json` is a snapshot that can be rebuilt from that log. A corrupt snapshot or a crash between the two writes loses nothing.
- **Ratings.** Again is failed recall. Hard, Good and Easy are successful recalls. Failed maps to Again and Passed to Good. Keys 1 to 4 give all four. Speed never changes a rating.
- **Days.** Elapsed time is counted in study days that roll over at 4am local time, as in Anki.
- **Learning steps.** FSRS has no learning steps, so the app adds one 10-minute step for new and lapsed cards, the pattern Anki uses. Same-day reviews pass zero elapsed days, which makes the library use its short-term update, so they gain far less than a delayed recall. A card can come round at most three times a study day.
- **Fitting.** Personalize runs `compute_parameters` on a background thread. Training items end only on delayed reviews, as the library expects. The library silently returns its defaults below 8 usable items, so the app detects that and reports it as not enough data. Under 400 items a fit is labelled provisional. If fitting fails, the previous parameters stay. A successful fit replays memory from the raw log.
- **Migration.** No earlier version stored reviews, due dates or ease factors, so there is nothing to migrate. `memory_state_from_sm2` does not apply. Every existing card starts as new, no outcomes are assumed, and the record says so.

## Ordinary scheduling

This is the default. Cards on a learning step come first, then due reviews (weakest first), then new cards up to the daily limit. Intervals come from FSRS at the configurable desired retention. Flagged cards wait behind everything else. When nothing is due, Space offers extra practice, which records real early reviews at their true timing.

## Deadline policy

**Objective.** Maximise predicted importance-weighted recall at the exam, subject to the estimated study time left. This is recall in the model. It is not a prediction of marks.

**Method.** Lagrangian relaxation over bounded, model-based lookahead, in `src-tauri/src/study/planner.rs`.

1. Each future day's study seconds get a price. With prices fixed, every card becomes its own small decision problem.
2. For each importance level, backward induction runs over a grid of stability and difficulty. At each decision day a card can stop, or pick its next review day. Each option is valued over Again, Hard, Good and Easy using the library's own transitions, with each outcome's time cost and everything that follows from it. Again includes the relearning step.
3. Unseen cards start at zero recall. Introducing one earns its later recall and pays for the reviews it creates.
4. Prices rise on days the cards overspend. This dual ascent shares a budget across decks and stops reviews piling up before an exam.
5. Today's list ranks cards by what a review is worth per second over the best alternative of waiting or stopping, then fills the time left. Cards the plan skips are not shown even if FSRS calls them due, and cards it picks are shown early. Their FSRS due dates are not changed, so turning deadline mode off resumes ordinary review exactly.

Two more terms shape behaviour.

- **Spare time is not free.** A review must earn at least `MIN_PRICE` per second. At neutral importance a 12-second review must add about 0.6 points of exam recall.
- **Planned reviews may not happen.** Without this, waiting always looks better because a later review is closer to the exam, and the plan procrastinates. Deferred gains are discounted by `DEFERRAL_SURVIVAL`, 3% a day, which also lowers the recall estimate.

**Budget.** There is one daily estimate. Per-deck minutes are reserved from it, scaled down if they promise more than exists, and the remaining decks share a pool. Expected ordinary-deck reviews come off that pool. Later days count at a reliability that starts at 70% and moves towards the use actually observed. Time already spent today comes off today. Review durations use defaults (18s new, 12s pass, 18s fail) blended with completed, uninterrupted reviews of 1 to 60 seconds. Hidden and unfocused time never counts.

**Rolling replan.** Planning runs off the review path and its result is cached. It reruns after completed reviews, budget, deadline or importance changes, flag changes and each new study day, which is how missed days are absorbed. Until a plan is ready the deck uses ordinary order.

**Deadlines.** The stored exam keeps the local wall-clock choice, its IANA time zone and the resulting instant. A date with no time uses a visible 09:00. After the exam passes, the deck uses ordinary review and the date stays until the player reschedules or resumes. Nothing is deleted.

**Coverage.** Flagged cards leave rapid review but stay in the estimates at their predicted recall, so flagging cannot make readiness look better. Unseen cards count as zero.

## Limitations

- This is an approximation with no optimality guarantee. State is interpolated on a coarse grid, days far from today and the exam are grouped, and a whole day is one decision.
- It models recall at one moment. It says nothing about understanding, or about how long knowledge lasts after the exam.
- `MIN_PRICE`, `DEFERRAL_SURVIVAL` and the rating shares are judgment calls, not fitted values. Rating shares do move with real use.
- FSRS treats each card independently. Related cards do not help each other in the model.
- Daily capacity is met in expectation by averaged prices. Today's list is exact, later days are not.
- Estimates assume the memory model fits the player. Before personalization that is an average of other people.

See [simulation-results.md](simulation-results.md) for the comparison with ordinary FSRS.


## Folders and overlay selection

Folders are library groups stored in `study/state.json`; deck JSON files stay in the flat flashcards directory, preserving file identities and card history. The home screen creates and renames folders, moves decks with a folder selector, and deletes empty folders. Existing course-style deck names such as `COMP3506 - Graphs` are automatically grouped once under `COMP3506`. Explicit moves to Unfiled are retained on reload.

Each deck has its own overlay checkbox, and each folder has an overlay switch. A deck appears only when both are enabled. Disabling a folder preserves its decks' individual choices. Disabled decks are excluded from Tab cycling, deadline planning and estimated ordinary review load. Their paused session snapshots, flipped cards and review history remain intact. When no decks are selected, the overlay explains how to select them in home.

A folder deadline is inherited by members without a deck override. A saved deck deadline takes precedence, including an explicitly disabled deadline. The deck screen shows inheritance and offers **Use folder deadline** to remove an override. Turning a folder deadline off leaves deck overrides alone; moving a deck changes its inherited policy. Folder minutes are reserved once and shared across enabled inheriting decks rather than multiplied by the number of decks. Advanced new-card caps remain a per-deck setting.

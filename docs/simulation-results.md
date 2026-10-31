# Deadline mode against ordinary FSRS

**Result:** deadline planning beat ordinary FSRS on model-predicted exam recall in every tested scenario and learner, but this is a simulation of FSRS-shaped learners. It does not establish that deadline mode works better for real people. Ordinary FSRS stays the default, and deadline mode is opt-in per deck.

## What the numbers show

- **Largest gains are when time is short.** Weighted recall rose 21 to 23 points with short deadlines, missed days and unequal importance. The main driver is coverage: ordinary FSRS spends its time on due reviews and leaves many cards unseen, while the planner reached 90% or more.
- **Importance works as a weight.** With unequal importance, weighted recall rose 22.5 points against 15.6 unweighted, so the planner does favour important cards.
- **Some of the gain is extra study time.** The planner used its available time more fully where time was not the limit. With ample time it studied 115 minutes against 75 for +3.6 points. On the long deadline it studied 457 against 412 for +2.7 points. Ordinary FSRS stops when nothing is due, and that may suit a player better when an exam is far off.
- **When time is far too short, recall stays low.** With 600 cards and about 65 minutes studied, the planner reached 23% against 16%. It helps, but no plan makes that fit, and the app says so.
- **A higher new-card limit does not close the gap.** Removing the limit barely changed ordinary FSRS, because due reviews already crowd out new cards.
- **Concentration fell.** The most-reviewed tenth of cards took a smaller share of reviews under deadline mode in every scenario. Over 1,296 runs there were no budget violations, and no card was reviewed more than 43 times.
- **Mismatch narrowed the gain but did not reverse it.** The planner always assumed default parameters. The gain was smallest for the differently shaped forgetting curve (+9.7 against +11.7 when matched). Individual runs did lose: 3 of 432 paired runs against the default baseline and 6 of 432 against the uncapped baseline, all with sparse history or interrupted sessions.

## How it was run

Each paired run gives the three policies the same learner, cards, starting state, available time, interruptions and luck. Random draws come from streams keyed by what they decide, so one policy's choices cannot change another's luck. Evaluation seeds decide the learner. Planning seeds only break ties in the planner. The planner sees only ratings, as the app would, and never the learner's true memory.

Learners are `Matched` (the planner's assumed parameters), `FastForgetter`, `SlowForgetter` and `SteeperCurve` (a different forgetting-curve decay). Each card also varies randomly in durability. Every day has irregular time, between half and all of the daily budget, split into slots of 15 to 45 seconds. A slot can end mid-card, which grades nothing and resumes later. Scenarios add missed days, sparse prior history, unequal importance and unseen material.

The scenarios and seeds were fixed before the final run. A short run on separate seeds (900 and 901) was used to check the simulator. It led to adding the stronger uncapped baseline, and changed nothing in the planner. No scenario or planner setting was changed after the final results were seen. The planner's `MIN_PRICE` and `DEFERRAL_SURVIVAL` were set earlier to fix behaviour caught by unit tests, before any comparison had run.

## Limitations

- Every learner is an FSRS model with changed parameters. Evaluation under the same family of models cannot establish real-world superiority, and real forgetting may differ in ways not tested here.
- The rating habits, review durations and downtime patterns are assumptions written for this simulator.
- The planner uses default time costs here (12 to 18 seconds) while simulated reviews take about 10 to 16, and it does not learn from them as the app does.
- Recall is measured at the exam moment only. A policy tuned for that moment may leave less long-term memory afterwards, and that was not measured.
- Intervals are over paired runs within this simulator, so they describe simulation noise, not uncertainty about real learners.
- The extra-practice option is not simulated. Both policies stop when they have nothing scheduled.

## Reproducing

From `src-tauri`, the run below used 4 threads. The other flags optimise only the crates that matter, because the machine lacked the disk space for a full release build.

```bash
cargo run --example compare_schedulers --config 'profile.dev.package.downtime.opt-level=3' --config 'profile.dev.package.fsrs.opt-level=3' -- --seeds 12 --first-seed 0 --threads 4
```

## Results

Paired runs: 12 seeds starting at 0, 9 scenarios, 4 learners.
Differences are deadline minus ordinary, with a 95% interval over paired runs.

### By scenario, all learners

| | Ordinary weighted recall | Deadline weighted recall | Difference in weighted recall (points) | Difference in unweighted recall (points) | Ordinary coverage | Deadline coverage | Ordinary top-decile share | Deadline top-decile share | Study minutes (ord / deadline) | Deadline wins |
|---|---|---|---|---|---|---|---|---|---|---|
| short deadline | 54.6% | 75.9% | +21.3 ± 1.1 | +21.3 ± 1.1 | 57.7% | 90.3% | 32.2% | 17.2% | 47 / 50 | 48/48 |
| long deadline | 94.6% | 97.4% | +2.7 ± 0.3 | +2.7 ± 0.3 | 99.6% | 100.0% | 26.5% | 20.5% | 412 / 457 | 48/48 |
| sparse history | 83.2% | 92.8% | +9.6 ± 1.6 | +9.6 ± 1.6 | 87.9% | 100.0% | 35.6% | 23.3% | 94 / 94 | 46/48 |
| missed days | 58.0% | 79.6% | +21.6 ± 1.6 | +21.6 ± 1.6 | 61.8% | 99.6% | 36.1% | 17.4% | 99 / 99 | 48/48 |
| unequal importance | 62.0% | 84.5% | +22.5 ± 1.4 | +15.6 ± 1.4 | 65.2% | 91.1% | 34.6% | 21.3% | 94 / 94 | 48/48 |
| long, unequal importance | 94.6% | 97.5% | +2.9 ± 0.3 | +1.9 ± 0.3 | 99.6% | 100.0% | 26.5% | 21.7% | 412 / 457 | 48/48 |
| mostly unseen, too little time | 16.0% | 23.2% | +7.2 ± 0.5 | +7.2 ± 0.5 | 17.0% | 29.6% | 77.2% | 47.3% | 65 / 65 | 48/48 |
| interrupted sessions | 81.7% | 92.0% | +10.3 ± 1.9 | +10.3 ± 1.9 | 86.4% | 100.0% | 28.5% | 17.3% | 94 / 94 | 47/48 |
| ample time | 95.7% | 99.3% | +3.6 ± 0.4 | +3.6 ± 0.4 | 100.0% | 100.0% | 23.5% | 19.0% | 75 / 115 | 48/48 |

### By learner, all scenarios

| | Ordinary weighted recall | Deadline weighted recall | Difference in weighted recall (points) | Difference in unweighted recall (points) | Ordinary coverage | Deadline coverage | Ordinary top-decile share | Deadline top-decile share | Study minutes (ord / deadline) | Deadline wins |
|---|---|---|---|---|---|---|---|---|---|---|
| Matched | 71.7% | 83.5% | +11.7 ± 1.7 | +11.0 ± 1.6 | 75.6% | 90.2% | 35.2% | 22.8% | 154 / 169 | 108/108 |
| FastForgetter | 67.4% | 80.5% | +13.1 ± 1.7 | +12.1 ± 1.6 | 72.7% | 90.0% | 37.9% | 22.9% | 161 / 170 | 108/108 |
| SlowForgetter | 74.7% | 85.3% | +10.6 ± 1.8 | +10.0 ± 1.6 | 77.6% | 90.3% | 32.8% | 22.4% | 147 / 169 | 106/108 |
| SteeperCurve | 70.7% | 80.5% | +9.7 ± 1.5 | +8.7 ± 1.4 | 74.2% | 89.9% | 36.5% | 23.0% | 156 / 169 | 107/108 |

### Scenario by learner: weighted recall difference (points)

| | Matched | FastForgetter | SlowForgetter | SteeperCurve |
|---|---|---|---|---|
| short deadline | +22.2 ± 2.1 | +21.0 ± 2.1 | +23.0 ± 2.2 | +18.9 ± 2.3 |
| long deadline | +3.2 ± 0.4 | +3.0 ± 1.2 | +2.6 ± 0.2 | +2.1 ± 0.4 |
| sparse history | +9.8 ± 2.5 | +13.4 ± 2.5 | +4.9 ± 2.6 | +10.3 ± 3.3 |
| missed days | +22.6 ± 2.8 | +25.4 ± 1.7 | +21.3 ± 3.7 | +17.1 ± 2.3 |
| unequal importance | +23.3 ± 2.5 | +24.6 ± 2.7 | +22.7 ± 2.3 | +19.3 ± 3.0 |
| long, unequal importance | +3.3 ± 0.3 | +3.4 ± 1.1 | +2.7 ± 0.2 | +2.2 ± 0.5 |
| mostly unseen, too little time | +7.4 ± 1.0 | +7.6 ± 1.0 | +7.8 ± 0.9 | +5.9 ± 1.0 |
| interrupted sessions | +10.3 ± 3.3 | +14.6 ± 3.7 | +7.7 ± 4.0 | +8.6 ± 3.4 |
| ample time | +3.6 ± 0.6 | +4.9 ± 0.9 | +2.9 ± 0.4 | +3.3 ± 0.5 |

### Against ordinary FSRS with no new-card limit

| | Uncapped weighted recall | Deadline weighted recall | Difference in weighted recall (points) | Difference in unweighted recall (points) | Uncapped coverage | Deadline coverage | Uncapped top-decile share | Study minutes (uncapped / deadline) | Deadline wins |
|---|---|---|---|---|---|---|---|---|---|
| short deadline | 55.2% | 75.9% | +20.6 ± 1.2 | +20.6 ± 1.2 | 58.4% | 90.3% | 32.2% | 50 / 50 | 48/48 |
| long deadline | 94.7% | 97.4% | +2.7 ± 0.4 | +2.7 ± 0.4 | 99.7% | 100.0% | 26.9% | 419 / 457 | 48/48 |
| sparse history | 83.2% | 92.8% | +9.6 ± 1.6 | +9.6 ± 1.6 | 87.9% | 100.0% | 35.6% | 94 / 94 | 46/48 |
| missed days | 58.6% | 79.6% | +20.9 ± 1.7 | +20.9 ± 1.7 | 62.5% | 99.6% | 35.9% | 99 / 99 | 48/48 |
| unequal importance | 62.0% | 84.5% | +22.5 ± 1.4 | +15.6 ± 1.4 | 65.1% | 91.1% | 34.2% | 94 / 94 | 48/48 |
| long, unequal importance | 94.7% | 97.5% | +2.8 ± 0.4 | +1.9 ± 0.4 | 99.7% | 100.0% | 26.9% | 419 / 457 | 48/48 |
| mostly unseen, too little time | 15.9% | 23.2% | +7.2 ± 0.5 | +7.2 ± 0.5 | 17.0% | 29.6% | 77.1% | 65 / 65 | 48/48 |
| interrupted sessions | 82.0% | 92.0% | +10.0 ± 1.8 | +10.0 ± 1.8 | 86.7% | 100.0% | 28.3% | 94 / 94 | 44/48 |
| ample time | 95.4% | 99.3% | +4.0 ± 0.4 | +4.0 ± 0.4 | 100.0% | 100.0% | 23.5% | 77 / 115 | 48/48 |

### Against no new-card limit, scenario by learner: weighted recall difference (points)

| | Matched | FastForgetter | SlowForgetter | SteeperCurve |
|---|---|---|---|---|
| short deadline | +21.0 ± 2.5 | +20.9 ± 2.3 | +21.6 ± 2.5 | +19.0 ± 2.5 |
| long deadline | +2.9 ± 0.3 | +2.9 ± 1.2 | +2.7 ± 0.2 | +2.3 ± 0.6 |
| sparse history | +9.8 ± 2.5 | +13.4 ± 2.5 | +4.8 ± 2.5 | +10.3 ± 3.3 |
| missed days | +21.9 ± 3.4 | +25.1 ± 2.2 | +20.4 ± 3.5 | +16.4 ± 2.7 |
| unequal importance | +23.0 ± 2.6 | +24.8 ± 2.8 | +22.7 ± 2.8 | +19.5 ± 2.7 |
| long, unequal importance | +2.9 ± 0.4 | +3.3 ± 1.1 | +2.7 ± 0.2 | +2.3 ± 0.8 |
| mostly unseen, too little time | +7.6 ± 0.9 | +7.5 ± 0.8 | +7.8 ± 0.9 | +6.0 ± 0.8 |
| interrupted sessions | +10.0 ± 3.4 | +14.0 ± 3.4 | +7.4 ± 3.5 | +8.7 ± 3.8 |
| ample time | +3.9 ± 0.6 | +5.4 ± 0.9 | +3.0 ± 0.4 | +3.6 ± 0.6 |

Budget violations across all runs: 0 days. Most reviews of any one card: 43.

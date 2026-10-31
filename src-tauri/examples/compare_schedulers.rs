//! Reproduces the tables in docs/simulation-results.md. Run from src-tauri:
//!
//!   cargo run --release --example compare_schedulers -- --threads 4
//!
//! With little disk space, optimise only the two crates that matter:
//!
//!   cargo run --example compare_schedulers --config 'profile.dev.package.downtime.opt-level=3' \
//!     --config 'profile.dev.package.fsrs.opt-level=3' -- --threads 4
//!
//! Pass `--seeds N`, `--first-seed N` and `--threads N` for other runs, and
//! `--out PATH` to choose where the report goes. Planning and evaluation seeds come from separate ranges.

use std::fmt::Write as _;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

use downtime_lib::study::simulation::{run, scenarios, Learner, Metrics, Policy, Scenario};

const PLAN_SEED_BASE: u64 = 1_000_000;
const EVAL_SEED_BASE: u64 = 2_000_000;

struct Pair {
    scenario: usize,
    learner: Learner,
    ordinary: Metrics,
    uncapped: Metrics,
    deadline: Metrics,
}

struct Interval {
    mean: f64,
    half_width: f64,
}

fn interval(values: &[f64]) -> Interval {
    let count = values.len() as f64;
    let mean = values.iter().sum::<f64>() / count;
    let variance = values.iter().map(|value| (value - mean).powi(2)).sum::<f64>() / (count - 1.0).max(1.0);
    Interval {
        mean,
        half_width: 1.96 * (variance / count).sqrt(),
    }
}

fn argument(name: &str, default: u64) -> u64 {
    let arguments: Vec<String> = std::env::args().collect();
    arguments
        .iter()
        .position(|argument| argument == name)
        .and_then(|index| arguments.get(index + 1))
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn out_path() -> Option<String> {
    let arguments: Vec<String> = std::env::args().collect();
    let index = arguments.iter().position(|argument| argument == "--out")?;
    arguments.get(index + 1).cloned()
}

fn run_all(scenarios: &[Scenario], seeds: u64, first_seed: u64) -> Vec<Pair> {
    let mut jobs = Vec::new();
    for scenario in 0..scenarios.len() {
        for learner in Learner::ALL {
            for seed in 0..seeds {
                jobs.push((scenario, learner, first_seed + seed));
            }
        }
    }
    let next = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(jobs.len()));
    let all_cores = thread::available_parallelism().map_or(4, |count| count.get()) as u64;
    let workers = argument("--threads", all_cores);
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let Some(&(index, learner, seed)) = jobs.get(next.fetch_add(1, Ordering::SeqCst)) else {
                    break;
                };
                let scenario = &scenarios[index];
                let (plan_seed, eval_seed) = (PLAN_SEED_BASE + seed, EVAL_SEED_BASE + seed);
                let ordinary = run(scenario, learner, Policy::Ordinary, plan_seed, eval_seed);
                let uncapped = run(scenario, learner, Policy::OrdinaryUncapped, plan_seed, eval_seed);
                let deadline = run(scenario, learner, Policy::Deadline, plan_seed, eval_seed);
                results.lock().unwrap().push(Pair { scenario: index, learner, ordinary, uncapped, deadline });
            });
        }
    });
    results.into_inner().unwrap()
}

fn mean(pairs: &[&Pair], value: impl Fn(&Pair) -> f64) -> f64 {
    pairs.iter().map(|pair| value(pair)).sum::<f64>() / pairs.len() as f64
}

fn points(value: f64) -> String {
    format!("{:.1}%", value * 100.0)
}

fn signed(value: Interval) -> String {
    format!("{:+.1} ± {:.1}", value.mean * 100.0, value.half_width * 100.0)
}

fn row(report: &mut String, label: &str, pairs: &[&Pair]) {
    let weighted: Vec<f64> = pairs
        .iter()
        .map(|pair| f64::from(pair.deadline.weighted_recall - pair.ordinary.weighted_recall))
        .collect();
    let plain: Vec<f64> = pairs
        .iter()
        .map(|pair| f64::from(pair.deadline.recall - pair.ordinary.recall))
        .collect();
    let wins = weighted.iter().filter(|&&difference| difference > 0.0).count();
    writeln!(
        report,
        "| {label} | {} | {} | {} | {} | {} | {} | {} | {} | {:.0} / {:.0} | {}/{} |",
        points(mean(pairs, |pair| f64::from(pair.ordinary.weighted_recall))),
        points(mean(pairs, |pair| f64::from(pair.deadline.weighted_recall))),
        signed(interval(&weighted)),
        signed(interval(&plain)),
        points(mean(pairs, |pair| f64::from(pair.ordinary.coverage))),
        points(mean(pairs, |pair| f64::from(pair.deadline.coverage))),
        points(mean(pairs, |pair| f64::from(pair.ordinary.top_decile_share))),
        points(mean(pairs, |pair| f64::from(pair.deadline.top_decile_share))),
        mean(pairs, |pair| f64::from(pair.ordinary.study_minutes)),
        mean(pairs, |pair| f64::from(pair.deadline.study_minutes)),
        wins,
        pairs.len(),
    )
    .unwrap();
}

const HEADER: &str = "| | Ordinary weighted recall | Deadline weighted recall | Difference in weighted recall (points) | Difference in unweighted recall (points) | Ordinary coverage | Deadline coverage | Ordinary top-decile share | Deadline top-decile share | Study minutes (ord / deadline) | Deadline wins |\n|---|---|---|---|---|---|---|---|---|---|---|";

fn main() {
    let seeds = argument("--seeds", 12);
    let first_seed = argument("--first-seed", 0);
    let scenarios = scenarios();
    let pairs = run_all(&scenarios, seeds, first_seed);

    let mut report = String::new();
    writeln!(report, "Paired runs: {seeds} seeds starting at {first_seed}, {} scenarios, {} learners.", scenarios.len(), Learner::ALL.len()).unwrap();
    writeln!(report, "Differences are deadline minus ordinary, with a 95% interval over paired runs.\n").unwrap();

    writeln!(report, "### By scenario, all learners\n\n{HEADER}").unwrap();
    for (index, scenario) in scenarios.iter().enumerate() {
        let group: Vec<&Pair> = pairs.iter().filter(|pair| pair.scenario == index).collect();
        row(&mut report, scenario.name, &group);
    }

    writeln!(report, "\n### By learner, all scenarios\n\n{HEADER}").unwrap();
    for learner in Learner::ALL {
        let group: Vec<&Pair> = pairs.iter().filter(|pair| pair.learner == learner).collect();
        row(&mut report, &format!("{learner:?}"), &group);
    }

    writeln!(report, "\n### Scenario by learner: weighted recall difference (points)\n").unwrap();
    let names: Vec<String> = Learner::ALL.iter().map(|learner| format!("{learner:?}")).collect();
    writeln!(report, "| | {} |\n|---|{}", names.join(" | "), "---|".repeat(names.len())).unwrap();
    for (index, scenario) in scenarios.iter().enumerate() {
        let cells: Vec<String> = Learner::ALL
            .iter()
            .map(|&learner| {
                let values: Vec<f64> = pairs
                    .iter()
                    .filter(|pair| pair.scenario == index && pair.learner == learner)
                    .map(|pair| f64::from(pair.deadline.weighted_recall - pair.ordinary.weighted_recall))
                    .collect();
                signed(interval(&values))
            })
            .collect();
        writeln!(report, "| {} | {} |", scenario.name, cells.join(" | ")).unwrap();
    }

    writeln!(report, "\n### Against ordinary FSRS with no new-card limit\n").unwrap();
    writeln!(report, "| | Uncapped weighted recall | Deadline weighted recall | Difference in weighted recall (points) | Difference in unweighted recall (points) | Uncapped coverage | Deadline coverage | Uncapped top-decile share | Study minutes (uncapped / deadline) | Deadline wins |\n|---|---|---|---|---|---|---|---|---|---|").unwrap();
    for (index, scenario) in scenarios.iter().enumerate() {
        let group: Vec<&Pair> = pairs.iter().filter(|pair| pair.scenario == index).collect();
        let weighted: Vec<f64> = group
            .iter()
            .map(|pair| f64::from(pair.deadline.weighted_recall - pair.uncapped.weighted_recall))
            .collect();
        let plain: Vec<f64> = group
            .iter()
            .map(|pair| f64::from(pair.deadline.recall - pair.uncapped.recall))
            .collect();
        writeln!(
            report,
            "| {} | {} | {} | {} | {} | {} | {} | {} | {:.0} / {:.0} | {}/{} |",
            scenario.name,
            points(mean(&group, |pair| f64::from(pair.uncapped.weighted_recall))),
            points(mean(&group, |pair| f64::from(pair.deadline.weighted_recall))),
            signed(interval(&weighted)),
            signed(interval(&plain)),
            points(mean(&group, |pair| f64::from(pair.uncapped.coverage))),
            points(mean(&group, |pair| f64::from(pair.deadline.coverage))),
            points(mean(&group, |pair| f64::from(pair.uncapped.top_decile_share))),
            mean(&group, |pair| f64::from(pair.uncapped.study_minutes)),
            mean(&group, |pair| f64::from(pair.deadline.study_minutes)),
            weighted.iter().filter(|&&difference| difference > 0.0).count(),
            group.len(),
        )
        .unwrap();
    }

    writeln!(report, "\n### Against no new-card limit, scenario by learner: weighted recall difference (points)\n").unwrap();
    writeln!(report, "| | {} |\n|---|{}", names.join(" | "), "---|".repeat(names.len())).unwrap();
    for (index, scenario) in scenarios.iter().enumerate() {
        let cells: Vec<String> = Learner::ALL
            .iter()
            .map(|&learner| {
                let values: Vec<f64> = pairs
                    .iter()
                    .filter(|pair| pair.scenario == index && pair.learner == learner)
                    .map(|pair| f64::from(pair.deadline.weighted_recall - pair.uncapped.weighted_recall))
                    .collect();
                signed(interval(&values))
            })
            .collect();
        writeln!(report, "| {} | {} |", scenario.name, cells.join(" | ")).unwrap();
    }

    let over: u32 = pairs
        .iter()
        .map(|pair| pair.ordinary.over_budget_days + pair.uncapped.over_budget_days + pair.deadline.over_budget_days)
        .sum();
    let highest = pairs
        .iter()
        .map(|pair| pair.deadline.max_reviews_per_card.max(pair.ordinary.max_reviews_per_card))
        .max()
        .unwrap_or(0);
    writeln!(report, "\nBudget violations across all runs: {over} days. Most reviews of any one card: {highest}.").unwrap();

    println!("{report}");
    if let Some(path) = out_path() {
        std::fs::write(path, &report).expect("the report path is writable");
    }
}

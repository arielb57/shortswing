//! Benchmarks: solve time on large histories with and without capacity
//! scaling, and LIHO's shortfall against the optimum on many small histories.
//!
//! Run with `cargo run --release --example bench [TRADES]` (default 1000).
//! Every history is generated
//! from a fixed seed, so the matchings and totals are identical on any machine;
//! only the timings vary.

use std::time::{Duration, Instant};

use shortswing::generate::{history, HistoryConfig};
use shortswing::{lowest_in_highest_out, solve, verify, Scaling, SixCalendarMonths};

fn median(mut v: Vec<Duration>) -> Duration {
    v.sort();
    v[v.len() / 2]
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    sorted[((sorted.len() - 1) as f64 * q).round() as usize]
}

fn main() {
    let rule = SixCalendarMonths;
    let runs = 3;
    let n: usize = std::env::args().nth(1).map(|a| a.parse().expect("trade count")).unwrap_or(1_000);
    println!("## Solve time, {n} trades (median of {runs} runs)\n");
    println!("| span | max shares | pair arcs | scaling | phases | augmentations | solve | verify | total profit |");
    println!("|---|---|---|---|---|---|---|---|---|");
    for &(span_days, max_shares) in &[(3650u32, 1_000u64), (3650, 10_000_000), (1460, 10_000_000)] {
        let config = HistoryConfig { trades: n, max_shares, span_days, ..HistoryConfig::default() };
        let trades = history(2026, &config);
        let mut totals = Vec::new();
        for scaling in [Scaling::On, Scaling::Off] {
            let mut times = Vec::new();
            let mut last = None;
            for _ in 0..runs {
                let t = Instant::now();
                let sol = solve(&trades, &rule, scaling).unwrap();
                times.push(t.elapsed());
                last = Some(sol);
            }
            let sol = last.unwrap();
            let t = Instant::now();
            let checked = verify(&trades, &rule, &sol.pairs, sol.total, &sol.certificate);
            let verify_time = t.elapsed();
            assert_eq!(checked, Ok(sol.total), "certificate must verify");
            totals.push(sol.total);
            println!(
                "| {} y | {} | {} | {} | {} | {} | {:.3} s | {:.3} s | {} ticks |",
                span_days / 365,
                max_shares,
                sol.stats.pair_arcs,
                if scaling == Scaling::On { "on" } else { "off" },
                sol.stats.phases,
                sol.stats.augmentations,
                median(times).as_secs_f64(),
                verify_time.as_secs_f64(),
                sol.total
            );
        }
        assert_eq!(totals[0], totals[1], "both strategies must reach the same optimum");
    }

    let histories = 1000;
    let config = HistoryConfig { trades: 60, max_shares: 20_000, span_days: 730, ..HistoryConfig::default() };
    let mut shortfall_pct = Vec::new();
    let mut strictly_below = 0;
    let mut with_profit = 0;
    let mut worst = (0.0f64, 0u64);
    for seed in 0..histories {
        let trades = history(seed, &config);
        let opt = solve(&trades, &rule, Scaling::On).unwrap();
        let greedy = lowest_in_highest_out(&trades, &rule);
        assert!(greedy.total <= opt.total);
        if opt.total == 0 {
            continue;
        }
        with_profit += 1;
        if greedy.total < opt.total {
            strictly_below += 1;
        }
        let pct = 100.0 * (opt.total - greedy.total) as f64 / opt.total as f64;
        if pct > worst.0 {
            worst = (pct, seed);
        }
        shortfall_pct.push(pct);
    }
    shortfall_pct.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean = shortfall_pct.iter().sum::<f64>() / shortfall_pct.len() as f64;
    println!("\n## LIHO shortfall, {histories} histories of {} trades over {} days\n", config.trades, config.span_days);
    println!("| histories with profit | LIHO strictly below optimum | mean | median | p90 | p99 | max (seed) |");
    println!("|---|---|---|---|---|---|---|");
    println!(
        "| {} | {} ({:.1}%) | {:.2}% | {:.2}% | {:.2}% | {:.2}% | {:.2}% ({}) |",
        with_profit,
        strictly_below,
        100.0 * strictly_below as f64 / with_profit as f64,
        mean,
        percentile(&shortfall_pct, 0.5),
        percentile(&shortfall_pct, 0.9),
        percentile(&shortfall_pct, 0.99),
        worst.0,
        worst.1
    );
}

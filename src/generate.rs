//! Deterministic synthetic insider trading histories.

use crate::date::Date;
use crate::trade::{Side, Trade};

/// SplitMix64: tiny, seedable, and good enough for test data.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `lo..=hi`.
    pub fn range(&mut self, lo: u64, hi: u64) -> u64 {
        lo + self.next_u64() % (hi - lo + 1)
    }
}

#[derive(Clone, Debug)]
pub struct HistoryConfig {
    pub trades: usize,
    /// Share counts are drawn log-uniformly from `1..=max_shares`.
    pub max_shares: u64,
    /// Calendar days the history spans.
    pub span_days: u32,
    /// First trading day.
    pub start: Date,
    /// Starting price in ticks.
    pub start_price: i64,
    /// Largest daily price move in ticks (uniform in ±this).
    pub daily_move: i64,
}

impl Default for HistoryConfig {
    fn default() -> HistoryConfig {
        HistoryConfig {
            trades: 50,
            max_shares: 10_000,
            span_days: 730,
            start: Date { year: 2020, month: 1, day: 2 },
            start_price: 500_000,
            daily_move: 5_000,
        }
    }
}

/// A random-walk share price with trades on random days. Insiders tend to
/// trade in runs (a series of sales under a plan, a burst of purchases), so the
/// side is only re-drawn for one trade in three.
pub fn history(seed: u64, config: &HistoryConfig) -> Vec<Trade> {
    let mut rng = Rng::new(seed);
    let span = config.span_days.max(1) as u64;
    let mut days: Vec<u64> = (0..config.trades).map(|_| rng.range(0, span - 1)).collect();
    days.sort_unstable();

    let start_day = config.start.day_number();
    let mut price = config.start_price;
    let mut price_day = 0u64;
    let mut side = Side::Buy;
    let max_bits = 64 - config.max_shares.max(1).leading_zeros() as u64;
    let mut trades = Vec::with_capacity(config.trades);
    for (i, &d) in days.iter().enumerate() {
        while price_day < d {
            let step = rng.range(0, 2 * config.daily_move as u64) as i64 - config.daily_move;
            price = (price + step).max(1);
            price_day += 1;
        }
        if i == 0 || rng.range(0, 2) == 0 {
            side = if rng.range(0, 1) == 0 { Side::Buy } else { Side::Sell };
        }
        let bits = rng.range(1, max_bits);
        let shares = rng.range(1u64 << (bits - 1), ((1u64 << bits) - 1).min(config.max_shares.max(1)));
        let intraday = rng.range(0, config.daily_move as u64) as i64 - config.daily_move / 2;
        trades.push(Trade {
            id: format!("T{}", i + 1),
            date: Date::from_day_number(start_day + d as i64),
            side,
            shares,
            price: (price + intraday).max(1),
        });
    }
    trades
}

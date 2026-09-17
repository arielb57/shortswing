#![allow(dead_code)]

use shortswing::generate::Rng;
use shortswing::{within, Date, Side, Trade, WindowRule};

pub fn trade(id: &str, date: &str, side: Side, shares: u64, price: i64) -> Trade {
    Trade { id: id.to_string(), date: Date::parse(date).unwrap(), side, shares, price }
}

/// A small random instance: dates cluster within about a year so that the
/// six-month boundary cuts through many pairs.
pub fn small_instance(rng: &mut Rng, max_trades: u64, max_shares: u64) -> Vec<Trade> {
    let n = rng.range(0, max_trades) as usize;
    let base = Date::new(2023, 6, 30).unwrap().day_number();
    (0..n)
        .map(|i| Trade {
            id: format!("T{i}"),
            date: Date::from_day_number(base + rng.range(0, 400) as i64),
            side: if rng.range(0, 1) == 0 { Side::Buy } else { Side::Sell },
            shares: rng.range(1, max_shares),
            price: rng.range(1, 12) as i64,
        })
        .collect()
}

/// Best total over every integer matching, found by enumerating the number of
/// shares on each in-window purchase/sale pair. Unprofitable pairs are
/// enumerated too; the oracle does not share the solver's pruning.
pub fn brute_force(trades: &[Trade], rule: &dyn WindowRule) -> i128 {
    let mut pairs = Vec::new();
    for (p, bt) in trades.iter().enumerate() {
        for (s, st) in trades.iter().enumerate() {
            if bt.side == Side::Buy && st.side == Side::Sell && within(rule, bt.date, st.date) {
                pairs.push((p, s, st.price as i128 - bt.price as i128));
            }
        }
    }
    let mut left: Vec<u64> = trades.iter().map(|t| t.shares).collect();
    fn go(k: usize, pairs: &[(usize, usize, i128)], left: &mut Vec<u64>) -> i128 {
        if k == pairs.len() {
            return 0;
        }
        let (p, s, gain) = pairs[k];
        let most = left[p].min(left[s]);
        let mut best = i128::MIN;
        for x in 0..=most {
            left[p] -= x;
            left[s] -= x;
            best = best.max(x as i128 * gain + go(k + 1, pairs, left));
            left[p] += x;
            left[s] += x;
        }
        best
    }
    go(0, &pairs, &mut left)
}

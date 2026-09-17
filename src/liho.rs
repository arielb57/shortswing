//! The spreadsheet baseline: lowest-in, highest-out.
//!
//! Purchases are taken cheapest first. Each one is matched against the
//! highest-priced sales still available inside its window, as many shares as
//! possible, before moving on. Ties break by date and then by input order.

use crate::date::WindowRule;
use crate::trade::{pair_gain, MatchedPair, Side, Trade};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GreedyResult {
    pub pairs: Vec<MatchedPair>,
    pub total: i128,
}

pub fn lowest_in_highest_out(trades: &[Trade], rule: &dyn WindowRule) -> GreedyResult {
    let mut buys: Vec<usize> = (0..trades.len()).filter(|&i| trades[i].side == Side::Buy).collect();
    let mut sells: Vec<usize> = (0..trades.len()).filter(|&i| trades[i].side == Side::Sell).collect();
    buys.sort_by_key(|&i| (trades[i].price, trades[i].date, i));
    sells.sort_by_key(|&i| (std::cmp::Reverse(trades[i].price), trades[i].date, i));

    let mut left: Vec<u64> = trades.iter().map(|t| t.shares).collect();
    let mut pairs = Vec::new();
    let mut total: i128 = 0;
    for &p in &buys {
        for &s in &sells {
            if left[p] == 0 {
                break;
            }
            if left[s] == 0 {
                continue;
            }
            if let Some(gain) = pair_gain(rule, &trades[p], &trades[s]) {
                let shares = left[p].min(left[s]);
                left[p] -= shares;
                left[s] -= shares;
                let profit = shares as i128 * gain as i128;
                total += profit;
                pairs.push(MatchedPair { purchase: p, sale: s, shares, profit });
            }
        }
    }
    GreedyResult { pairs, total }
}

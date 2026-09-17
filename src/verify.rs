//! Independent optimality check.
//!
//! This module does not look at the solver's network or potentials. It
//! recomputes which pairs are matchable directly from the trades and the
//! window rule, then checks a claimed matching and dual prices against the LP
//!
//! ```text
//! maximise   sum gain(p,s) * x(p,s)
//! subject to sum_s x(p,s) <= shares(p),  sum_p x(p,s) <= shares(s),  x >= 0
//! ```
//!
//! and its dual (`u_p, v_s >= 0`, `u_p + v_s >= gain(p,s)`). A primal-feasible
//! matching and a dual-feasible certificate with equal objectives are both
//! optimal; that equality is exactly complementary slackness.

use std::fmt;

use crate::date::{Date, WindowRule};
use crate::solver::Certificate;
use crate::trade::{pair_gain, MatchedPair, Side, Trade};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Violation {
    CertificateLength { expected: usize, found: usize },
    BadIndex { pair: usize },
    NotMatchable { pair: usize },
    ZeroShares { pair: usize },
    WrongProfit { pair: usize, claimed: i128, actual: i128 },
    Overmatched { trade: usize, matched: u128, available: u64 },
    NegativeDual { trade: usize },
    DualInfeasible { purchase: usize, sale: usize, gain: i64, dual_sum: i128 },
    SlackPair { pair: usize, gain: i64, dual_sum: i128 },
    SlackTrade { trade: usize, dual: i64, matched: u128, available: u64 },
    TotalMismatch { claimed: i128, actual: i128 },
    DualityGap { primal: i128, dual: i128 },
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Violation::CertificateLength { expected, found } => {
                write!(f, "certificate has {found} duals, expected {expected}")
            }
            Violation::BadIndex { pair } => write!(f, "pair {pair} does not join a purchase to a sale"),
            Violation::NotMatchable { pair } => {
                write!(f, "pair {pair} is outside the window or not profitable")
            }
            Violation::ZeroShares { pair } => write!(f, "pair {pair} matches zero shares"),
            Violation::WrongProfit { pair, claimed, actual } => {
                write!(f, "pair {pair} claims profit {claimed}, actual {actual}")
            }
            Violation::Overmatched { trade, matched, available } => {
                write!(f, "trade {trade} matched {matched} shares but has {available}")
            }
            Violation::NegativeDual { trade } => write!(f, "trade {trade} has a negative dual"),
            Violation::DualInfeasible { purchase, sale, gain, dual_sum } => {
                write!(f, "duals of purchase {purchase} and sale {sale} sum to {dual_sum} < gain {gain}")
            }
            Violation::SlackPair { pair, gain, dual_sum } => {
                write!(f, "pair {pair} is matched but its duals sum to {dual_sum}, not gain {gain}")
            }
            Violation::SlackTrade { trade, dual, matched, available } => {
                write!(f, "trade {trade} has dual {dual} > 0 but only {matched} of {available} shares matched")
            }
            Violation::TotalMismatch { claimed, actual } => {
                write!(f, "claimed total {claimed} but pairs sum to {actual}")
            }
            Violation::DualityGap { primal, dual } => {
                write!(f, "matching profit {primal} is below the dual bound {dual}")
            }
        }
    }
}

/// Verifies that `pairs` is a feasible matching with total `claimed_total`
/// and that `certificate` proves no matching can earn more.
/// Returns the verified total.
pub fn verify(
    trades: &[Trade],
    rule: &dyn WindowRule,
    pairs: &[MatchedPair],
    claimed_total: i128,
    certificate: &Certificate,
) -> Result<i128, Violation> {
    let n = trades.len();
    if certificate.duals.len() != n {
        return Err(Violation::CertificateLength { expected: n, found: certificate.duals.len() });
    }

    let mut matched = vec![0u128; n];
    let mut primal: i128 = 0;
    for (k, pair) in pairs.iter().enumerate() {
        if pair.purchase >= n || pair.sale >= n {
            return Err(Violation::BadIndex { pair: k });
        }
        let (p, s) = (&trades[pair.purchase], &trades[pair.sale]);
        if p.side != Side::Buy || s.side != Side::Sell {
            return Err(Violation::BadIndex { pair: k });
        }
        let Some(gain) = pair_gain(rule, p, s) else {
            return Err(Violation::NotMatchable { pair: k });
        };
        if pair.shares == 0 {
            return Err(Violation::ZeroShares { pair: k });
        }
        let actual = pair.shares as i128 * gain as i128;
        if actual != pair.profit {
            return Err(Violation::WrongProfit { pair: k, claimed: pair.profit, actual });
        }
        matched[pair.purchase] += pair.shares as u128;
        matched[pair.sale] += pair.shares as u128;
        primal += actual;
    }
    for (i, t) in trades.iter().enumerate() {
        if matched[i] > t.shares as u128 {
            return Err(Violation::Overmatched { trade: i, matched: matched[i], available: t.shares });
        }
    }
    if primal != claimed_total {
        return Err(Violation::TotalMismatch { claimed: claimed_total, actual: primal });
    }

    if let Some(i) = certificate.duals.iter().position(|&d| d < 0) {
        return Err(Violation::NegativeDual { trade: i });
    }
    let window_end: Vec<Date> = trades.iter().map(|t| rule.window_end(t.date)).collect();
    let buys: Vec<usize> = (0..n).filter(|&i| trades[i].side == Side::Buy).collect();
    let sells: Vec<usize> = (0..n).filter(|&i| trades[i].side == Side::Sell).collect();
    for &pi in &buys {
        for &si in &sells {
            let (p, s) = (&trades[pi], &trades[si]);
            let in_window = if p.date <= s.date { s.date < window_end[pi] } else { p.date < window_end[si] };
            if !in_window || s.price <= p.price {
                continue;
            }
            let gain = s.price - p.price;
            let dual_sum = certificate.duals[pi] as i128 + certificate.duals[si] as i128;
            if dual_sum < gain as i128 {
                return Err(Violation::DualInfeasible { purchase: pi, sale: si, gain, dual_sum });
            }
        }
    }

    // Complementary slackness, reported individually so a failure says where.
    for (k, pair) in pairs.iter().enumerate() {
        let gain = trades[pair.sale].price - trades[pair.purchase].price;
        let dual_sum = certificate.duals[pair.purchase] as i128 + certificate.duals[pair.sale] as i128;
        if dual_sum != gain as i128 {
            return Err(Violation::SlackPair { pair: k, gain, dual_sum });
        }
    }
    for (i, t) in trades.iter().enumerate() {
        let dual = certificate.duals[i];
        if dual > 0 && matched[i] != t.shares as u128 {
            return Err(Violation::SlackTrade { trade: i, dual, matched: matched[i], available: t.shares });
        }
    }

    let dual = certificate.dual_objective(trades);
    if primal != dual {
        return Err(Violation::DualityGap { primal, dual });
    }
    Ok(primal)
}

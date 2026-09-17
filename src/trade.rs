//! Transactions, prices in integer ticks, and matched pairs.

use crate::date::{within, Date, WindowRule};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

/// Largest accepted price in ticks. Keeps every intermediate potential in the
/// solver far away from `i64` overflow.
pub const MAX_PRICE_TICKS: i64 = 1_000_000_000_000;
/// Largest accepted share count for one trade.
pub const MAX_SHARES: u64 = 1_000_000_000_000;
/// Largest accepted number of trades in one history.
pub const MAX_TRADES: usize = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Trade {
    pub id: String,
    pub date: Date,
    pub side: Side,
    pub shares: u64,
    /// Price per share in integer ticks (for example 1 tick = $0.0001).
    pub price: i64,
}

/// A purchase matched against a sale. `purchase` and `sale` index into the trade slice.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchedPair {
    pub purchase: usize,
    pub sale: usize,
    pub shares: u64,
    /// `shares * (sale price - purchase price)` in ticks.
    pub profit: i128,
}

/// Checks the limits the solver relies on.
pub fn validate(trades: &[Trade]) -> Result<(), String> {
    if trades.len() > MAX_TRADES {
        return Err(format!("{} trades exceeds the limit of {MAX_TRADES}", trades.len()));
    }
    for t in trades {
        if t.shares == 0 || t.shares > MAX_SHARES {
            return Err(format!("trade {}: shares must be in 1..={MAX_SHARES}", t.id));
        }
        if t.price < 0 || t.price > MAX_PRICE_TICKS {
            return Err(format!("trade {}: price must be in 0..={MAX_PRICE_TICKS} ticks", t.id));
        }
    }
    Ok(())
}

/// Per-share profit if `purchase` and `sale` may be matched and the sale price
/// is strictly higher; `None` otherwise.
pub fn pair_gain(rule: &dyn WindowRule, purchase: &Trade, sale: &Trade) -> Option<i64> {
    if purchase.side != Side::Buy || sale.side != Side::Sell {
        return None;
    }
    if sale.price <= purchase.price || !within(rule, purchase.date, sale.date) {
        return None;
    }
    Some(sale.price - purchase.price)
}

/// Parses a decimal price such as `12.5` into ticks with `decimals` fractional digits.
/// Rejects prices that are not an exact number of ticks rather than rounding.
pub fn parse_price(s: &str, decimals: u32) -> Result<i64, String> {
    let s = s.trim();
    let (int_part, frac_part) = match s.split_once('.') {
        Some((i, f)) => (i, f),
        None => (s, ""),
    };
    let digits = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit());
    if !digits(int_part) || (!frac_part.is_empty() && !digits(frac_part)) || s.ends_with('.') {
        return Err(format!("price {s:?} is not a non-negative decimal number"));
    }
    let trimmed = frac_part.trim_end_matches('0');
    if trimmed.len() as u32 > decimals {
        return Err(format!("price {s:?} has more than {decimals} decimal places"));
    }
    let scale = 10i128.pow(decimals);
    let whole: i128 = int_part.parse::<i128>().map_err(|_| format!("price {s:?} is too large"))?;
    let mut frac: i128 = 0;
    if !trimmed.is_empty() {
        frac = trimmed.parse::<i128>().map_err(|e| e.to_string())? * 10i128.pow(decimals - trimmed.len() as u32);
    }
    let ticks = whole
        .checked_mul(scale)
        .and_then(|w| w.checked_add(frac))
        .filter(|&t| t <= MAX_PRICE_TICKS as i128)
        .ok_or_else(|| format!("price {s:?} is too large"))?;
    Ok(ticks as i64)
}

/// Formats ticks back to a decimal string with `decimals` fractional digits.
pub fn format_ticks(ticks: i128, decimals: u32) -> String {
    if decimals == 0 {
        return ticks.to_string();
    }
    let scale = 10i128.pow(decimals);
    let sign = if ticks < 0 { "-" } else { "" };
    let abs = ticks.abs();
    format!("{sign}{}.{:0width$}", abs / scale, abs % scale, width = decimals as usize)
}

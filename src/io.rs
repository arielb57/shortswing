//! CSV input and JSON output, without external crates.

use std::fmt::Write as _;

use crate::date::{Date, WindowRule};
use crate::liho::GreedyResult;
use crate::solver::Solution;
use crate::trade::{format_ticks, parse_price, MatchedPair, Side, Trade};

fn parse_side(s: &str) -> Result<Side, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "buy" | "b" | "purchase" | "p" => Ok(Side::Buy),
        "sell" | "s" | "sale" => Ok(Side::Sell),
        other => Err(format!("side {other:?} is not buy or sell")),
    }
}

fn unquote(field: &str) -> &str {
    let f = field.trim();
    f.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(f)
}

/// Reads transactions from CSV text.
///
/// The first non-empty line is a header naming the columns `date`, `side`,
/// `shares`, `price` and optionally `id`, in any order. Rows without an `id`
/// column are named `T1`, `T2`, ... in file order. Lines starting with `#` are
/// ignored. Fields may be wrapped in double quotes but may not contain commas.
pub fn parse_csv(text: &str, tick_decimals: u32) -> Result<Vec<Trade>, String> {
    let mut lines = text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
    let Some((_, header)) = lines.next() else {
        return Err("CSV is empty".to_string());
    };
    let names: Vec<String> = header.split(',').map(|h| unquote(h).to_ascii_lowercase()).collect();
    let col = |name: &str| names.iter().position(|n| n == name);
    let missing = |name: &str| format!("CSV header has no {name:?} column");
    let date_col = col("date").ok_or_else(|| missing("date"))?;
    let side_col = col("side").ok_or_else(|| missing("side"))?;
    let shares_col = col("shares").ok_or_else(|| missing("shares"))?;
    let price_col = col("price").ok_or_else(|| missing("price"))?;
    let id_col = col("id");

    let mut trades = Vec::new();
    for (line_no, line) in lines {
        let fields: Vec<&str> = line.split(',').map(unquote).collect();
        let at = |e: String| format!("line {}: {e}", line_no + 1);
        if fields.len() != names.len() {
            return Err(at(format!("expected {} fields, found {}", names.len(), fields.len())));
        }
        let shares: u64 = fields[shares_col]
            .parse()
            .map_err(|_| at(format!("shares {:?} is not a whole number", fields[shares_col])))?;
        let id = match id_col {
            Some(c) => fields[c].to_string(),
            None => format!("T{}", trades.len() + 1),
        };
        trades.push(Trade {
            id,
            date: Date::parse(fields[date_col]).map_err(at)?,
            side: parse_side(fields[side_col]).map_err(at)?,
            shares,
            price: parse_price(fields[price_col], tick_decimals).map_err(at)?,
        });
    }
    let mut seen = std::collections::HashSet::new();
    for t in &trades {
        if !seen.insert(t.id.as_str()) {
            return Err(format!("trade id {:?} appears more than once", t.id));
        }
    }
    Ok(trades)
}

pub fn write_csv(trades: &[Trade], tick_decimals: u32) -> String {
    let mut out = String::from("id,date,side,shares,price\n");
    for t in trades {
        let side = match t.side {
            Side::Buy => "buy",
            Side::Sell => "sell",
        };
        let price = format_ticks(t.price as i128, tick_decimals);
        let _ = writeln!(out, "{},{},{},{},{}", t.id, t.date, side, t.shares, price);
    }
    out
}

pub fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn pairs_json(out: &mut String, trades: &[Trade], pairs: &[MatchedPair], dec: u32, indent: &str) {
    if pairs.is_empty() {
        out.push_str("[]");
        return;
    }
    out.push_str("[\n");
    for (k, p) in pairs.iter().enumerate() {
        let (b, s) = (&trades[p.purchase], &trades[p.sale]);
        let _ = write!(
            out,
            "{indent}  {{\"purchase_id\": {}, \"purchase_date\": \"{}\", \"purchase_price\": \"{}\", \
             \"sale_id\": {}, \"sale_date\": \"{}\", \"sale_price\": \"{}\", \"shares\": {}, \"profit\": \"{}\"}}",
            json_string(&b.id),
            b.date,
            format_ticks(b.price as i128, dec),
            json_string(&s.id),
            s.date,
            format_ticks(s.price as i128, dec),
            p.shares,
            format_ticks(p.profit, dec),
        );
        out.push_str(if k + 1 < pairs.len() { ",\n" } else { "\n" });
    }
    let _ = write!(out, "{indent}]");
}

/// The CLI report. Money amounts are decimal strings so that totals beyond
/// 2^53 survive JSON parsers that read numbers as doubles.
pub fn report_json(
    trades: &[Trade],
    rule: &dyn WindowRule,
    dec: u32,
    solution: &Solution,
    verified: &Result<i128, String>,
    greedy: &GreedyResult,
) -> String {
    let mut out = String::from("{\n");
    let _ = writeln!(out, "  \"window_rule\": {},", json_string(&rule.name()));
    let _ = writeln!(out, "  \"tick_decimals\": {dec},");
    out.push_str("  \"pairs\": ");
    pairs_json(&mut out, trades, &solution.pairs, dec, "  ");
    out.push_str(",\n");
    let _ = writeln!(out, "  \"total_profit\": \"{}\",", format_ticks(solution.total, dec));
    out.push_str("  \"certificate\": {\n    \"duals_per_share\": [\n");
    for (i, t) in trades.iter().enumerate() {
        let side = if t.side == Side::Buy { "buy" } else { "sell" };
        let _ = write!(
            out,
            "      {{\"id\": {}, \"side\": \"{side}\", \"shares\": {}, \"dual\": \"{}\"}}",
            json_string(&t.id),
            t.shares,
            format_ticks(solution.certificate.duals[i] as i128, dec)
        );
        out.push_str(if i + 1 < trades.len() { ",\n" } else { "\n" });
    }
    out.push_str("    ],\n");
    let _ = writeln!(
        out,
        "    \"dual_objective\": \"{}\",",
        format_ticks(solution.certificate.dual_objective(trades), dec)
    );
    match verified {
        Ok(_) => out.push_str("    \"verified\": true\n"),
        Err(e) => {
            let _ = writeln!(out, "    \"verified\": false,\n    \"violation\": {}", json_string(e));
        }
    }
    out.push_str("  },\n  \"liho\": {\n    \"pairs\": ");
    pairs_json(&mut out, trades, &greedy.pairs, dec, "    ");
    let _ = writeln!(out, ",\n    \"total_profit\": \"{}\"", format_ticks(greedy.total, dec));
    out.push_str("  },\n");
    let _ = writeln!(out, "  \"liho_shortfall\": \"{}\"", format_ticks(solution.total - greedy.total, dec));
    out.push_str("}\n");
    out
}

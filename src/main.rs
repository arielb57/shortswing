use std::process::ExitCode;

use shortswing::generate::{history, HistoryConfig};
use shortswing::io::{parse_csv, report_json, write_csv};
use shortswing::{lowest_in_highest_out, solve, verify, Date, DayCount, Scaling, SixCalendarMonths, WindowRule};

const USAGE: &str = "\
shortswing - largest Section 16(b) short-swing profit, with an optimality certificate

USAGE:
    shortswing match <FILE.csv|-> [--decimals N] [--window six-months|days:N] [--no-scaling]
    shortswing generate [--trades N] [--seed S] [--max-shares M] [--span-days D] [--decimals N]

match     Reads transactions (header: date,side,shares,price[,id]) and prints the
          optimal pairs, total, dual certificate and the LIHO baseline as JSON.
          Exit code 2 if the certificate fails verification.
generate  Prints a synthetic insider trading history as CSV.

--decimals N   price ticks per unit are 10^N (default 4, so 1 tick = 0.0001)";

fn value<'a>(args: &'a [String], i: &mut usize, flag: &str) -> Result<&'a str, String> {
    *i += 1;
    args.get(*i).map(|s| s.as_str()).ok_or_else(|| format!("{flag} needs a value"))
}

fn number<T: std::str::FromStr>(s: &str, flag: &str) -> Result<T, String> {
    s.parse().map_err(|_| format!("{flag}: {s:?} is not a valid number"))
}

fn run_match(args: &[String]) -> Result<ExitCode, String> {
    let mut path = None;
    let mut decimals = 4u32;
    let mut rule: Box<dyn WindowRule> = Box::new(SixCalendarMonths);
    let mut scaling = Scaling::On;
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--decimals" => decimals = number(value(args, &mut i, "--decimals")?, "--decimals")?,
            "--no-scaling" => scaling = Scaling::Off,
            "--window" => {
                let w = value(args, &mut i, "--window")?;
                rule = if w == "six-months" {
                    Box::new(SixCalendarMonths)
                } else if let Some(d) = w.strip_prefix("days:") {
                    Box::new(DayCount { days: number(d, "--window")? })
                } else {
                    return Err(format!("unknown window rule {w:?}"));
                };
            }
            other if path.is_none() && (!other.starts_with("--") || other == "-") => path = Some(other.to_string()),
            other => return Err(format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    if decimals > 9 {
        return Err("--decimals must be at most 9".to_string());
    }
    let path = path.ok_or("match needs a CSV file (or - for stdin)")?;
    let text = if path == "-" {
        std::io::read_to_string(std::io::stdin()).map_err(|e| e.to_string())?
    } else {
        std::fs::read_to_string(&path).map_err(|e| format!("{path}: {e}"))?
    };
    let trades = parse_csv(&text, decimals)?;
    let solution = solve(&trades, rule.as_ref(), scaling)?;
    let verified = verify(&trades, rule.as_ref(), &solution.pairs, solution.total, &solution.certificate)
        .map_err(|v| v.to_string());
    let greedy = lowest_in_highest_out(&trades, rule.as_ref());
    print!("{}", report_json(&trades, rule.as_ref(), decimals, &solution, &verified, &greedy));
    Ok(if verified.is_ok() { ExitCode::SUCCESS } else { ExitCode::from(2) })
}

fn run_generate(args: &[String]) -> Result<ExitCode, String> {
    let mut config = HistoryConfig::default();
    let mut seed = 1u64;
    let mut decimals = 4u32;
    let mut i = 0;
    while i < args.len() {
        let flag = args[i].as_str();
        match flag {
            "--trades" => config.trades = number(value(args, &mut i, flag)?, flag)?,
            "--seed" => seed = number(value(args, &mut i, flag)?, flag)?,
            "--max-shares" => config.max_shares = number(value(args, &mut i, flag)?, flag)?,
            "--span-days" => config.span_days = number(value(args, &mut i, flag)?, flag)?,
            "--decimals" => decimals = number(value(args, &mut i, flag)?, flag)?,
            "--start" => config.start = Date::parse(value(args, &mut i, flag)?)?,
            other => return Err(format!("unexpected argument {other:?}")),
        }
        i += 1;
    }
    if config.max_shares == 0 || config.span_days == 0 || decimals > 9 {
        return Err("--max-shares and --span-days must be positive, --decimals at most 9".to_string());
    }
    print!("{}", write_csv(&history(seed, &config), decimals));
    Ok(ExitCode::SUCCESS)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(|s| s.as_str()) {
        Some("match") => run_match(&args[1..]),
        Some("generate") => run_generate(&args[1..]),
        Some("-h") | Some("--help") | Some("help") => {
            println!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        _ => Err(USAGE.to_string()),
    };
    match result {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

use std::io::Write;
use std::process::{Command, Stdio};

use shortswing::io::parse_csv;
use shortswing::trade::parse_price;

fn run(args: &[&str], stdin: &str) -> (i32, String, String) {
    let mut child = Command::new(env!("CARGO_BIN_EXE_shortswing"))
        .args(args)
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(stdin.as_bytes()).unwrap();
    let out = child.wait_with_output().unwrap();
    (out.status.code().unwrap_or(-1), String::from_utf8(out.stdout).unwrap(), String::from_utf8(out.stderr).unwrap())
}

#[test]
fn match_reports_optimum_certificate_and_liho_shortfall() {
    let (code, out, err) = run(&["match", "examples/liho_counterexample.csv"], "");
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"total_profit\": \"1200.0000\""), "{out}");
    assert!(out.contains("\"verified\": true"), "{out}");
    assert!(out.contains("\"dual_objective\": \"1200.0000\""), "{out}");
    assert!(out.contains("\"liho_shortfall\": \"150.0000\""), "{out}");
    assert!(out.contains("\"purchase_id\": \"B2\", \"purchase_date\": \"2024-11-01\""), "{out}");
}

#[test]
fn generate_output_feeds_match() {
    let (code, csv, err) = run(&["generate", "--trades", "80", "--seed", "3"], "");
    assert_eq!(code, 0, "{err}");
    assert_eq!(csv.lines().count(), 81);
    assert_eq!(parse_csv(&csv, 4).unwrap().len(), 80);
    let (code, out, err) = run(&["match", "-", "--no-scaling"], &csv);
    assert_eq!(code, 0, "{err}");
    assert!(out.contains("\"verified\": true"));
    let (_, again, _) = run(&["generate", "--trades", "80", "--seed", "3"], "");
    assert_eq!(csv, again, "generator must be deterministic for a seed");
}

#[test]
fn window_flag_changes_matching() {
    let csv = "date,side,shares,price\n2024-01-01,buy,10,1\n2024-03-15,sell,10,2\n";
    let (_, six, _) = run(&["match", "-"], csv);
    assert!(six.contains("\"total_profit\": \"10.0000\""), "{six}");
    let (_, short, _) = run(&["match", "-", "--window", "days:30"], csv);
    assert!(short.contains("\"total_profit\": \"0.0000\""), "{short}");
    assert!(short.contains("\"window_rule\": \"days:30\""));
}

#[test]
fn bad_input_is_reported_not_panicked() {
    let cases = [
        ("date,side,shares,price\n2024-02-30,buy,1,1\n", "line 2"),
        ("date,side,shares,price\n2024-02-01,hold,1,1\n", "not buy or sell"),
        ("date,side,shares\n2024-02-01,buy,1\n", "\"price\""),
        ("date,side,shares,price\n2024-02-01,buy,0,1\n", "shares must be"),
        ("date,side,shares,price\n2024-02-01,buy,1,1.00001\n", "decimal places"),
        ("id,date,side,shares,price\nA,2024-02-01,buy,1,1\nA,2024-02-02,sell,1,2\n", "more than once"),
    ];
    for (csv, needle) in cases {
        let (code, _, err) = run(&["match", "-"], csv);
        assert_eq!(code, 1, "{csv}");
        assert!(err.contains(needle), "{csv}: {err}");
    }
    let (code, _, err) = run(&["frobnicate"], "");
    assert_eq!(code, 1);
    assert!(err.contains("USAGE"));
}

#[test]
fn price_parsing_is_exact() {
    assert_eq!(parse_price("12.5", 4), Ok(125_000));
    assert_eq!(parse_price("0.0001", 4), Ok(1));
    assert_eq!(parse_price("7", 0), Ok(7));
    assert_eq!(parse_price("3.10000", 2), Ok(310));
    assert!(parse_price("-1", 4).is_err());
    assert!(parse_price("1.", 4).is_err());
    assert!(parse_price(".5", 4).is_err());
    assert!(parse_price("1e3", 4).is_err());
    assert!(parse_price("99999999999999999999", 4).is_err());
}

#[test]
fn csv_quoted_fields_comments_and_default_ids() {
    let csv = "# exported\n\"Date\",\"Side\",\"Shares\",\"Price\"\n\n\"2024-02-01\",\"B\",\"5\",\"1.25\"\n2024-02-02,sale,5,2\n";
    let trades = parse_csv(csv, 4).unwrap();
    assert_eq!(trades.len(), 2);
    assert_eq!(trades[0].id, "T1");
    assert_eq!(trades[1].id, "T2");
    assert_eq!(trades[0].price, 12_500);
}

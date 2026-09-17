mod common;

use common::trade;
use shortswing::{
    lowest_in_highest_out, solve, verify, within, Date, DayCount, Scaling, Side, SixCalendarMonths, Trade, WindowRule,
};

fn d(s: &str) -> Date {
    Date::parse(s).unwrap()
}

fn matched_total(trades: &[Trade]) -> i128 {
    solve(trades, &SixCalendarMonths, Scaling::On).unwrap().total
}

#[test]
fn jan_31_window_ends_jul_31() {
    let rule = SixCalendarMonths;
    assert_eq!(rule.window_end(d("2024-01-31")), d("2024-07-31"));
    assert!(within(&rule, d("2024-01-31"), d("2024-07-30")));
    assert!(!within(&rule, d("2024-01-31"), d("2024-07-31")));

    let buy = trade("B", "2024-01-31", Side::Buy, 10, 100);
    assert_eq!(matched_total(&[buy.clone(), trade("S", "2024-07-30", Side::Sell, 10, 150)]), 500);
    assert_eq!(matched_total(&[buy, trade("S", "2024-07-31", Side::Sell, 10, 150)]), 0);
}

#[test]
fn month_end_clamps_to_february() {
    let rule = SixCalendarMonths;
    assert_eq!(rule.window_end(d("2022-08-31")), d("2023-02-28"));
    assert_eq!(rule.window_end(d("2023-08-31")), d("2024-02-29"));
    assert_eq!(rule.window_end(d("2023-08-30")), d("2024-02-29"));
    assert_eq!(rule.window_end(d("2022-08-29")), d("2023-02-28"));
    assert!(within(&rule, d("2022-08-31"), d("2023-02-27")));
    assert!(!within(&rule, d("2022-08-31"), d("2023-02-28")));
    assert!(within(&rule, d("2023-08-31"), d("2024-02-28")));
    assert!(!within(&rule, d("2023-08-31"), d("2024-02-29")));
}

#[test]
fn leap_day_purchase() {
    let rule = SixCalendarMonths;
    assert_eq!(rule.window_end(d("2024-02-29")), d("2024-08-29"));
    let buy = trade("B", "2024-02-29", Side::Buy, 3, 10);
    assert_eq!(matched_total(&[buy.clone(), trade("S", "2024-08-28", Side::Sell, 3, 20)]), 30);
    assert_eq!(matched_total(&[buy, trade("S", "2024-08-29", Side::Sell, 3, 20)]), 0);
    assert!(Date::parse("2023-02-29").is_err());
}

#[test]
fn window_crosses_year_end() {
    let rule = SixCalendarMonths;
    assert_eq!(rule.window_end(d("2023-07-15")), d("2024-01-15"));
    assert_eq!(rule.window_end(d("2023-12-31")), d("2024-06-30"));
}

#[test]
fn same_day_buy_and_sell_match() {
    let trades = [trade("B", "2024-05-01", Side::Buy, 7, 100), trade("S", "2024-05-01", Side::Sell, 5, 104)];
    assert_eq!(matched_total(&trades), 20);
}

#[test]
fn sale_before_purchase_counts() {
    let trades = [trade("S", "2024-01-31", Side::Sell, 10, 150), trade("B", "2024-07-30", Side::Buy, 10, 100)];
    assert_eq!(matched_total(&trades), 500);
    let late = [trade("S", "2024-01-31", Side::Sell, 10, 150), trade("B", "2024-07-31", Side::Buy, 10, 100)];
    assert_eq!(matched_total(&late), 0);
}

#[test]
fn equal_or_lower_sale_price_is_not_profit() {
    let trades = [
        trade("B", "2024-05-01", Side::Buy, 10, 100),
        trade("S1", "2024-05-02", Side::Sell, 10, 100),
        trade("S2", "2024-05-03", Side::Sell, 10, 90),
    ];
    let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
    assert_eq!(sol.total, 0);
    assert!(sol.pairs.is_empty());
}

#[test]
fn day_count_rule_is_pluggable() {
    let rule = DayCount { days: 30 };
    assert!(within(&rule, d("2024-02-01"), d("2024-03-01")));
    assert!(!within(&rule, d("2024-02-01"), d("2024-03-02")));
    let trades = [trade("B", "2024-02-01", Side::Buy, 1, 1), trade("S", "2024-03-02", Side::Sell, 1, 9)];
    assert_eq!(solve(&trades, &rule, Scaling::On).unwrap().total, 0);
    assert_eq!(solve(&trades, &SixCalendarMonths, Scaling::On).unwrap().total, 8);
}

#[test]
fn pinned_counterexample_liho_strictly_below_optimum() {
    let trades = vec![
        trade("S1", "2024-01-02", Side::Sell, 100, 140_000),
        trade("B1", "2024-03-01", Side::Buy, 100, 100_000),
        trade("S2", "2024-06-03", Side::Sell, 100, 200_000),
        trade("B2", "2024-11-01", Side::Buy, 100, 120_000),
        trade("S3", "2024-12-16", Side::Sell, 50, 130_000),
    ];
    let rule = SixCalendarMonths;
    let greedy = lowest_in_highest_out(&trades, &rule);
    let sol = solve(&trades, &rule, Scaling::On).unwrap();
    assert_eq!(greedy.total, 10_500_000);
    assert_eq!(sol.total, 12_000_000);
    let mut pairs: Vec<(usize, usize, u64)> = sol.pairs.iter().map(|p| (p.purchase, p.sale, p.shares)).collect();
    pairs.sort();
    assert_eq!(pairs, vec![(1, 0, 100), (3, 2, 100)]);
    assert_eq!(verify(&trades, &rule, &sol.pairs, sol.total, &sol.certificate), Ok(12_000_000));
    // LIHO's matching cannot be proven optimal by any certificate.
    assert!(verify(&trades, &rule, &greedy.pairs, greedy.total, &sol.certificate).is_err());
}

#[test]
fn liho_is_optimal_without_window_conflicts() {
    // All trades within a month: the classic LIHO argument holds.
    let trades = vec![
        trade("B1", "2024-03-01", Side::Buy, 30, 10),
        trade("B2", "2024-03-02", Side::Buy, 50, 12),
        trade("S1", "2024-03-03", Side::Sell, 40, 15),
        trade("S2", "2024-03-04", Side::Sell, 60, 11),
    ];
    let greedy = lowest_in_highest_out(&trades, &SixCalendarMonths);
    assert_eq!(greedy.total, matched_total(&trades));
    assert_eq!(greedy.total, 30 * 5 + 10 * 3);
}

#[test]
fn date_round_trips_and_rejects_bad_input() {
    // 0001-01-02 through roughly the year 9900.
    for z in -719_161..2_900_000i64 {
        if z % 997 == 0 {
            let date = Date::from_day_number(z);
            assert_eq!(date.day_number(), z);
            assert_eq!(Date::parse(&date.to_string()), Ok(date));
        }
    }
    assert_eq!(Date::parse("1970-01-01").unwrap().day_number(), 0);
    assert_eq!(Date::parse("2000-03-01").unwrap().day_number(), 11_017);
    for bad in ["2024-13-01", "2024-04-31", "24-01-01", "2024/01/01", "2024-1-01", ""] {
        assert!(Date::parse(bad).is_err(), "{bad}");
    }
}

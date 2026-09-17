mod common;

use common::brute_force;
use proptest::prelude::*;
use shortswing::{lowest_in_highest_out, solve, verify, Date, Scaling, Side, SixCalendarMonths, Trade};

fn arb_trade() -> impl Strategy<Value = Trade> {
    (0i64..500, any::<bool>(), 1u64..=1_000, 1i64..10_000).prop_map(|(day, buy, shares, price)| Trade {
        id: String::new(),
        date: Date::from_day_number(Date::new(2022, 12, 31).unwrap().day_number() + day),
        side: if buy { Side::Buy } else { Side::Sell },
        shares,
        price,
    })
}

fn arb_history(max: usize) -> impl Strategy<Value = Vec<Trade>> {
    prop::collection::vec(arb_trade(), 0..max)
}

fn total(trades: &[Trade]) -> i128 {
    solve(trades, &SixCalendarMonths, Scaling::On).unwrap().total
}

fn config(cases: u32) -> ProptestConfig {
    // Shrinking re-runs the solver (and sometimes the brute-force oracle) per
    // step; cap it so a failure is reported in minutes rather than hours.
    ProptestConfig { cases, max_shrink_time: 60_000, max_shrink_iters: 2_000, ..ProptestConfig::default() }
}

proptest! {
    #![proptest_config(config(300))]

    #[test]
    fn adding_a_transaction_never_lowers_the_total(trades in arb_history(40), extra in arb_trade()) {
        let before = total(&trades);
        let mut more = trades.clone();
        more.push(extra);
        prop_assert!(total(&more) >= before);
    }

    #[test]
    fn shifting_every_price_leaves_the_total_unchanged(trades in arb_history(40), shift in 0i64..1_000_000) {
        let shifted: Vec<Trade> = trades.iter().map(|t| Trade { price: t.price + shift, ..t.clone() }).collect();
        prop_assert_eq!(total(&shifted), total(&trades));
    }

    #[test]
    fn scaling_shares_by_k_scales_the_total(trades in arb_history(40), k in 1u64..5_000) {
        let scaled: Vec<Trade> = trades.iter().map(|t| Trade { shares: t.shares * k, ..t.clone() }).collect();
        prop_assert_eq!(total(&scaled), total(&trades) * k as i128);
    }

    #[test]
    fn matched_shares_never_exceed_either_trade(trades in arb_history(60)) {
        for scaling in [Scaling::On, Scaling::Off] {
            let sol = solve(&trades, &SixCalendarMonths, scaling).unwrap();
            let mut used = vec![0u64; trades.len()];
            for p in &sol.pairs {
                prop_assert!(p.shares <= trades[p.purchase].shares.min(trades[p.sale].shares));
                used[p.purchase] += p.shares;
                used[p.sale] += p.shares;
            }
            for (u, t) in used.iter().zip(&trades) {
                prop_assert!(*u <= t.shares);
            }
            prop_assert_eq!(sol.pairs.iter().map(|p| p.profit).sum::<i128>(), sol.total);
            prop_assert_eq!(
                verify(&trades, &SixCalendarMonths, &sol.pairs, sol.total, &sol.certificate),
                Ok(sol.total)
            );
        }
    }

    #[test]
    fn scaling_and_plain_agree_and_beat_liho(trades in arb_history(60)) {
        let on = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
        let off = solve(&trades, &SixCalendarMonths, Scaling::Off).unwrap();
        prop_assert_eq!(on.total, off.total);
        prop_assert!(lowest_in_highest_out(&trades, &SixCalendarMonths).total <= on.total);
    }
}

proptest! {
    #![proptest_config(config(500))]

    #[test]
    fn tiny_instances_equal_brute_force(
        trades in prop::collection::vec(
            (0i64..400, any::<bool>(), 1u64..=5, 1i64..12).prop_map(|(day, buy, shares, price)| Trade {
                id: String::new(),
                date: Date::from_day_number(Date::new(2023, 8, 31).unwrap().day_number() + day),
                side: if buy { Side::Buy } else { Side::Sell },
                shares,
                price,
            }),
            0..=6,
        )
    ) {
        prop_assert_eq!(total(&trades), brute_force(&trades, &SixCalendarMonths));
    }
}

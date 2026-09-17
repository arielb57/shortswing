mod common;

use common::{small_instance, trade};
use shortswing::generate::{history, HistoryConfig, Rng};
use shortswing::{
    lowest_in_highest_out, solve, verify, Certificate, MatchedPair, Scaling, Side, SixCalendarMonths, Trade, Violation,
};

fn counterexample() -> Vec<Trade> {
    vec![
        trade("S1", "2024-01-02", Side::Sell, 100, 140_000),
        trade("B1", "2024-03-01", Side::Buy, 100, 100_000),
        trade("S2", "2024-06-03", Side::Sell, 100, 200_000),
        trade("B2", "2024-11-01", Side::Buy, 100, 120_000),
        trade("S3", "2024-12-16", Side::Sell, 50, 130_000),
    ]
}

#[test]
fn solver_output_verifies_on_synthetic_histories() {
    for seed in 0..40 {
        let config = HistoryConfig { trades: 120, max_shares: 50_000, ..HistoryConfig::default() };
        let trades = history(seed, &config);
        let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
        let plain = solve(&trades, &SixCalendarMonths, Scaling::Off).unwrap();
        assert_eq!(sol.total, plain.total, "seed {seed}");
        assert_eq!(verify(&trades, &SixCalendarMonths, &sol.pairs, sol.total, &sol.certificate), Ok(sol.total));
        assert_eq!(verify(&trades, &SixCalendarMonths, &plain.pairs, plain.total, &plain.certificate), Ok(plain.total));
    }
}

#[test]
fn suboptimal_liho_matching_fails_against_the_certificate() {
    let trades = counterexample();
    let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
    let greedy = lowest_in_highest_out(&trades, &SixCalendarMonths);
    let err = verify(&trades, &SixCalendarMonths, &greedy.pairs, greedy.total, &sol.certificate).unwrap_err();
    assert!(
        matches!(err, Violation::SlackPair { .. } | Violation::SlackTrade { .. } | Violation::DualityGap { .. }),
        "{err:?}"
    );
}

#[test]
fn moving_one_share_to_a_worse_pair_fails() {
    let trades = counterexample();
    let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
    // B2 -> S2 carries 100 shares at 8.00; divert one share to B2 -> S3 at 1.00.
    let mut pairs = sol.pairs.clone();
    let k = pairs.iter().position(|p| p.purchase == 3 && p.sale == 2).unwrap();
    pairs[k].shares -= 1;
    pairs[k].profit -= 80_000;
    pairs.push(MatchedPair { purchase: 3, sale: 4, shares: 1, profit: 10_000 });
    let total: i128 = pairs.iter().map(|p| p.profit).sum();
    assert!(verify(&trades, &SixCalendarMonths, &pairs, total, &sol.certificate).is_err());
}

#[test]
fn each_kind_of_corruption_is_rejected() {
    let trades = counterexample();
    let rule = SixCalendarMonths;
    let sol = solve(&trades, &rule, Scaling::On).unwrap();
    let ok = |pairs: &[MatchedPair], total: i128, cert: &Certificate| verify(&trades, &rule, pairs, total, cert);

    let mut over = sol.pairs.clone();
    over[0].shares += 1;
    over[0].profit += 40_000;
    let t: i128 = over.iter().map(|p| p.profit).sum();
    assert!(matches!(ok(&over, t, &sol.certificate), Err(Violation::Overmatched { .. })));

    // B1 (2024-03-01) and S3 (2024-12-16) are more than six months apart.
    let outside = vec![MatchedPair { purchase: 1, sale: 4, shares: 1, profit: 30_000 }];
    assert!(matches!(ok(&outside, 30_000, &sol.certificate), Err(Violation::NotMatchable { .. })));

    let reversed = vec![MatchedPair { purchase: 0, sale: 1, shares: 1, profit: 0 }];
    assert!(matches!(ok(&reversed, 0, &sol.certificate), Err(Violation::BadIndex { .. })));

    let mut lying = sol.pairs.clone();
    lying[0].profit += 1;
    assert!(matches!(ok(&lying, sol.total + 1, &sol.certificate), Err(Violation::WrongProfit { .. })));

    assert!(matches!(ok(&sol.pairs, sol.total + 1, &sol.certificate), Err(Violation::TotalMismatch { .. })));

    let mut low = sol.certificate.clone();
    low.duals[2] -= 1;
    assert!(matches!(ok(&sol.pairs, sol.total, &low), Err(Violation::DualInfeasible { .. })));

    let mut high = sol.certificate.clone();
    high.duals[4] += 1;
    assert!(matches!(ok(&sol.pairs, sol.total, &high), Err(Violation::SlackTrade { .. })));

    let mut negative = sol.certificate.clone();
    negative.duals[4] = -1;
    assert!(matches!(ok(&sol.pairs, sol.total, &negative), Err(Violation::NegativeDual { .. })));

    let short = Certificate { duals: vec![0; 3] };
    assert!(matches!(ok(&sol.pairs, sol.total, &short), Err(Violation::CertificateLength { .. })));

    // Leaving profit on the table with a generous but feasible dual is a duality gap.
    let generous = Certificate { duals: vec![100_000; 5] };
    assert!(ok(&[], 0, &generous).is_err());
}

#[test]
fn dropping_any_pair_from_an_optimal_matching_fails() {
    let mut rng = Rng::new(99);
    let mut tried = 0;
    while tried < 300 {
        let trades = small_instance(&mut rng, 6, 5);
        let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
        if sol.pairs.is_empty() {
            continue;
        }
        tried += 1;
        for k in 0..sol.pairs.len() {
            let mut pairs = sol.pairs.clone();
            let removed = pairs.remove(k);
            let total = sol.total - removed.profit;
            assert!(verify(&trades, &SixCalendarMonths, &pairs, total, &sol.certificate).is_err(), "{trades:#?}");
        }
    }
}

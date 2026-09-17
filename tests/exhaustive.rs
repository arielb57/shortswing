mod common;

use common::{brute_force, small_instance};
use shortswing::generate::Rng;
use shortswing::{solve, verify, DayCount, Scaling, SixCalendarMonths, WindowRule};

fn check_against_brute_force(rule: &dyn WindowRule, seed: u64, instances: usize) {
    let mut rng = Rng::new(seed);
    let mut nonzero = 0;
    for case in 0..instances {
        let trades = small_instance(&mut rng, 6, 5);
        let expected = brute_force(&trades, rule);
        for scaling in [Scaling::On, Scaling::Off] {
            let sol = solve(&trades, rule, scaling).unwrap();
            assert_eq!(sol.total, expected, "case {case} {scaling:?}: {trades:#?}");
            let checked = verify(&trades, rule, &sol.pairs, sol.total, &sol.certificate);
            assert_eq!(checked, Ok(expected), "case {case} {scaling:?}: {trades:#?}");
        }
        if expected > 0 {
            nonzero += 1;
        }
    }
    // Guards against a generator that only produces trivial instances.
    assert!(nonzero > instances / 5, "only {nonzero} of {instances} instances had profit");
}

#[test]
fn six_month_rule_matches_brute_force_on_5000_instances() {
    check_against_brute_force(&SixCalendarMonths, 0x5EED, 5000);
}

#[test]
fn day_count_rule_matches_brute_force_on_2000_instances() {
    check_against_brute_force(&DayCount { days: 90 }, 42, 2000);
}

#[test]
fn larger_shares_match_brute_force() {
    // Share counts up to 40 exercise several scaling phases on the same oracle.
    let mut rng = Rng::new(7);
    for case in 0..300 {
        let trades = small_instance(&mut rng, 5, 40);
        let expected = brute_force(&trades, &SixCalendarMonths);
        let sol = solve(&trades, &SixCalendarMonths, Scaling::On).unwrap();
        assert_eq!(sol.total, expected, "case {case}: {trades:#?}");
    }
}

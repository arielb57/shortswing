//! Largest Section 16(b) short-swing profit, with a dual certificate of optimality.
//!
//! ```
//! use shortswing::{solve, verify, Date, Scaling, Side, SixCalendarMonths, Trade};
//!
//! let t = |id: &str, date: &str, side, shares, price| Trade {
//!     id: id.into(), date: Date::parse(date).unwrap(), side, shares, price,
//! };
//! let trades = vec![
//!     t("B1", "2024-01-10", Side::Buy, 100, 1000),
//!     t("S1", "2024-03-01", Side::Sell, 60, 1500),
//!     t("S2", "2024-12-01", Side::Sell, 100, 1400),
//! ];
//! let rule = SixCalendarMonths;
//! let solution = solve(&trades, &rule, Scaling::On).unwrap();
//! assert_eq!(solution.total, 60 * 500);
//! let checked = verify(&trades, &rule, &solution.pairs, solution.total, &solution.certificate);
//! assert_eq!(checked, Ok(30_000));
//! ```

pub mod date;
pub mod generate;
pub mod io;
pub mod liho;
pub mod solver;
pub mod trade;
pub mod verify;

pub use date::{within, Date, DayCount, SixCalendarMonths, WindowRule};
pub use liho::{lowest_in_highest_out, GreedyResult};
pub use solver::{solve, Certificate, Scaling, Solution, SolveStats};
pub use trade::{MatchedPair, Side, Trade};
pub use verify::{verify, Violation};

# shortswing

Computes the largest Section 16(b) short-swing profit in an insider's trade history, and a certificate that proves no pairing of trades gives more.

## The problem

Section 16(b) of the Securities Exchange Act makes an officer, director or 10% holder give back any profit from a purchase and a sale (in either order) less than six months apart. Courts measure that profit by pairing purchases with sales so the total is as large as possible. In practice lawyers and compliance teams often do it in a spreadsheet with "lowest-in, highest-out" (LIHO): take the cheapest purchase, match it to the most expensive sale, and repeat. That is correct when every trade can pair with every other one. Once the six-month window rules out some pairs it can fall short, and a spreadsheet gives you no way to show that a figure is the maximum.

## How it works

**Model.** Matching is a transportation problem. Each purchase supplies its share count and each sale takes up to its share count. A purchase `p` and a sale `s` can be matched only when the later trade falls inside the window of the earlier one and the sale price is higher. Each share matched earns `price(s) - price(p)`. Prices are stored as integer ticks (10^-4 by default), so the arithmetic is exact.

**Solver** (`src/solver.rs`). The transportation problem becomes a min-cost circulation: a source feeds every purchase, every sale drains to a sink, pair arcs cost `-gain`, and a zero-cost return arc runs from sink to source. Sending no flow is always allowed, so pairs that lose money stay unmatched. The circulation is solved with **capacity-scaling successive shortest paths** (Ahuja, Magnanti & Orlin, §10.2). Every shortest-path search is a Dijkstra over reduced costs kept non-negative by **Johnson potentials**. Scaling means share counts in the millions cost only about log₂(max shares) phases, rather than one augmentation per unit of imbalance.

**Certificate** (`src/verify.rs`). When the solver finishes, its node potentials give one dual price per trade: `u_p` for each purchase and `v_s` for each sale. By LP weak duality, if `u_p, v_s ≥ 0` and `u_p + v_s ≥ gain(p, s)` for every matchable pair, then no matching can earn more than `Σ shares × dual`. The verifier does not trust the solver. From the raw trades and the window rule alone it:

1. checks that the matching is feasible (no trade over-matched, every pair in its window and profitable, profits add up),
2. checks that the duals are feasible for **every** matchable pair,
3. checks complementary slackness, and that the matching's profit equals the dual bound.

If all three hold, the total is optimal. A bug in the solver shows up as a failed check. It can never produce a wrong total that still verifies.

**Window rule** (`src/date.rs`). The default is "less than six calendar months", counted forward from the earlier trade. When the day does not exist in the later month it is clamped to the month's last day: Aug 31 counts forward to Feb 28, or Feb 29 in a leap year. Jan 31 → Jul 30 is inside the window and Jul 31 is not. The rule is a trait, and `--window days:N` swaps in a plain day count.

### Worked example

`examples/liho_counterexample.csv`:

```text
id,date,side,shares,price
S1,2024-01-02,sell,100,14.00
B1,2024-03-01,buy,100,10.00
S2,2024-06-03,sell,100,20.00
B2,2024-11-01,buy,100,12.00
S3,2024-12-16,sell,50,13.00
```

LIHO takes the cheapest purchase B1 ($10) and pairs it with the highest sale S2 ($20), earning $1,000. That leaves B2 ($12) with S1 out of its window and S2 used up. Its only partner is 50 shares of S3, for $50. Total: $1,050.

The optimum pairs B1 with S1 ($400) and B2 with S2 ($800), for $1,200. The certificate proves it. The duals are S1 = 1, B1 = 3, S2 = 7, B2 = 1 and S3 = 0 per share. Every matchable pair is covered: B1+S1 = 4 ≥ 4, B1+S2 = 10 ≥ 10, B2+S2 = 8 ≥ 8 and B2+S3 = 1 ≥ 1. The dual bound is 100·1 + 100·3 + 100·7 + 100·1 = 1,200, which equals the matching's profit.

## Install and usage

Requires a Rust toolchain (tested with 1.94) and has no runtime dependencies. From a checkout of this repository:

```sh
cargo build --release          # binary at ./target/release/shortswing
```

Compute the optimum, the certificate and the LIHO baseline. Input columns are `date,side,shares,price` plus an optional `id`, in any order. Use `-` to read from stdin.

```sh
cargo run --release -- match examples/liho_counterexample.csv
```

```json
{
  "window_rule": "six-calendar-months",
  "tick_decimals": 4,
  "pairs": [
    {"purchase_id": "B1", "purchase_date": "2024-03-01", "purchase_price": "10.0000", "sale_id": "S1", "sale_date": "2024-01-02", "sale_price": "14.0000", "shares": 100, "profit": "400.0000"},
    {"purchase_id": "B2", "purchase_date": "2024-11-01", "purchase_price": "12.0000", "sale_id": "S2", "sale_date": "2024-06-03", "sale_price": "20.0000", "shares": 100, "profit": "800.0000"}
  ],
  "total_profit": "1200.0000",
  "certificate": {
    "duals_per_share": [
      {"id": "S1", "side": "sell", "shares": 100, "dual": "1.0000"},
      {"id": "B1", "side": "buy", "shares": 100, "dual": "3.0000"},
      {"id": "S2", "side": "sell", "shares": 100, "dual": "7.0000"},
      {"id": "B2", "side": "buy", "shares": 100, "dual": "1.0000"},
      {"id": "S3", "side": "sell", "shares": 50, "dual": "0.0000"}
    ],
    "dual_objective": "1200.0000",
    "verified": true
  },
  "liho": {
    "pairs": [
      {"purchase_id": "B1", "purchase_date": "2024-03-01", "purchase_price": "10.0000", "sale_id": "S2", "sale_date": "2024-06-03", "sale_price": "20.0000", "shares": 100, "profit": "1000.0000"},
      {"purchase_id": "B2", "purchase_date": "2024-11-01", "purchase_price": "12.0000", "sale_id": "S3", "sale_date": "2024-12-16", "sale_price": "13.0000", "shares": 50, "profit": "50.0000"}
    ],
    "total_profit": "1050.0000"
  },
  "liho_shortfall": "150.0000"
}
```

The exit code is 0 when the certificate verifies, 2 when it does not, and 1 for bad input. Bad input includes impossible dates, unknown sides, zero shares, prices with more decimals than `--decimals` and duplicate ids. Each is reported with its line number.

Other options:

```sh
./target/release/shortswing match trades.csv --window days:180   # plain day-count window
./target/release/shortswing match trades.csv --decimals 2        # prices in cents
./target/release/shortswing generate --trades 200 | ./target/release/shortswing match - --no-scaling
```

Generate a synthetic history (a random-walk price, with trades in runs of buys or sells). The same seed always gives the same output:

```sh
cargo run --release -- generate --trades 6 --seed 7
```

```text
id,date,side,shares,price
T1,2020-03-26,buy,9,52.1271
T2,2020-06-06,sell,937,54.0342
T3,2020-12-10,sell,152,52.8445
T4,2021-01-13,sell,163,52.3225
T5,2021-06-19,sell,6446,57.2936
T6,2021-11-07,sell,467,58.2343
```

Run the tests and the benchmark:

```sh
cargo test
cargo run --release --example bench          # optional argument: trade count, default 1000
```

## Results

Measured with `cargo run --release --example bench` on an Apple Silicon (arm64) Mac, macOS 26, Rust 1.94.1. All histories come from fixed seeds, so the pair counts, augmentations and totals below are the same on any machine. Only the timings depend on hardware.

**Solve time, 1,000 trades (median of 3 runs).** Every solution was checked by the independent verifier, and both modes reach the same total.

| span | max shares | pair arcs | scaling | phases | augmentations | solve | verify | total profit |
|---|---|---|---|---|---|---|---|---|
| 10 y | 1000 | 12139 | on | 17 | 7001 | 0.085 s | 0.000 s | 1425481066 ticks |
| 10 y | 1000 | 12139 | off | 1 | 13939 | 0.669 s | 0.000 s | 1425481066 ticks |
| 10 y | 10000000 | 12139 | on | 29 | 7623 | 0.087 s | 0.000 s | 6519183310451 ticks |
| 10 y | 10000000 | 12139 | off | 1 | 13875 | 0.702 s | 0.000 s | 6519183310451 ticks |
| 4 y | 10000000 | 30739 | on | 29 | 16867 | 0.209 s | 0.000 s | 7536130524920 ticks |
| 4 y | 10000000 | 30739 | off | 1 | 32716 | 2.755 s | 0.000 s | 7536130524920 ticks |

Capacity scaling is 8–13× faster here. Verification takes under a millisecond, because it is one pass over the pairs. The solve time grows faster than linearly: 3,000 trades over 10 years (113,835 pair arcs) took 3.5 s with scaling. Pass a trade count to measure other sizes, e.g. `cargo run --release --example bench 3000`. Expect many minutes at that size, because the benchmark also runs plain SSP.

**How much LIHO misses: 1,000 synthetic histories of 60 trades over two years.**

| histories with profit | LIHO strictly below optimum | mean | median | p90 | p99 | max (seed) |
|---|---|---|---|---|---|---|
| 1000 | 468 (46.8%) | 0.60% | 0.00% | 1.57% | 11.91% | 21.58% (206) |

Shortfall is `(optimum − LIHO) / optimum`. LIHO usually gets close, but it is below the optimum in almost half of these histories, and in 1 in 100 it misses more than 12% of the recoverable profit. Nothing in its output tells you which case you are in.

## Tests

`cargo test` runs 32 tests and a doctest. The ones that carry weight:

- **Exhaustive oracle** (`tests/exhaustive.rs`). On 7,300 random small instances the solver's total matches a brute force that enumerates every integer matching, including unprofitable pairs. This covers both window rules and both solver modes. The certificate of each solution must also verify.
- **Properties** (`tests/properties.rs`, proptest). Adding a trade never lowers the total. Shifting every price by a constant leaves it unchanged. Multiplying every share count by k multiplies it by k. Scaling and plain SSP always agree, and LIHO is never above the optimum. Shrinking is capped at 60 s per failure.
- **Certificate tampering** (`tests/certificate.rs`). Lowered duals, inflated totals, over-matched trades, out-of-window pairs and slack pairs are each rejected with the specific violation.
- **Window edges** (`tests/window_and_liho.rs`). Month-end clamping, leap days, same-day trades, sales before purchases, and hand-built cases where LIHO is strictly below the optimum.
- **CLI** (`tests/cli.rs`). End-to-end runs of the binary, error messages for malformed CSV, and exact price parsing.

## Design notes

The main decision was to return a proof rather than just a number. An optimal matching can be computed in many ways. A figure in a 16(b) complaint or a settlement negotiation gets challenged, though, and "the program said so" is weak evidence. The dual certificate turns that into arithmetic anyone can recheck: one price per trade, one inequality per in-window pair, one sum. The verifier is written apart from the solver on purpose. It rebuilds the matchable pairs from the dates, never reads the flow network, and is short enough to audit. Because of that separation, the solver can use a fast and fairly intricate algorithm without making the result harder to trust.

The solver itself trades memory for simplicity. It builds a dense arc for every in-window profitable pair, which is O(n²) in the worst case, rather than a sparser structure that exploits date order. That keeps the network a plain textbook transportation problem that is easy to test against brute force, and a single insider's history in one security is usually hundreds of trades, not tens of thousands. Capacity scaling was added because share counts are large and uneven (9 shares next to 6 million). Plain successive shortest paths does one Dijkstra per augmentation, and the benchmark shows how quickly that grows.

## Limitations

- **Not legal advice, and not the whole of 16(b).** The tool computes the maximum matched profit on one class of equity security. It does not decide who counts as an insider, handle derivatives or conversion rules, apply exemptions (Rule 16b-3 grants, gifts), net out commissions or add interest. It also does not model the matching variations some courts use.
- **The window is an interpretation.** The default counts six calendar months forward from the earlier trade and clamps month-ends. Other readings can be plugged in through the `WindowRule` trait, or approximated with `--window days:N`, but they have not been checked against case law.
- **Quadratic arcs.** Memory and time grow with the number of in-window pairs. Histories of tens of thousands of trades concentrated in a few years will be slow, and plain SSP (`--no-scaling`) is impractical well before that.
- **Dates only.** Trades on the same day always match, and intraday order is ignored.
- **Simple CSV.** Fields may be quoted but may not contain commas. Prices must have at most `--decimals` decimal places, and nothing is rounded.

## License

MIT. See [LICENSE](LICENSE).

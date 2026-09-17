//! Maximum-profit matching as a min-cost circulation.
//!
//! Network: source `S`, sink `T`, one node per trade.
//! `S -> purchase` (capacity = shares, cost 0), `purchase -> sale` (cost = -gain),
//! `sale -> T` (capacity = shares, cost 0) and a return arc `T -> S` (cost 0).
//! A minimum-cost circulation is a maximum-profit matching; routing no flow is
//! always allowed, so unprofitable pairs are simply left unmatched.
//!
//! The algorithm is capacity-scaling successive shortest paths
//! (Ahuja, Magnanti & Orlin, section 10.2) with Johnson potentials so every
//! shortest-path search is a Dijkstra over non-negative reduced costs.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use crate::date::WindowRule;
use crate::trade::{validate, MatchedPair, Side, Trade};

/// Whether to run the capacity-scaling phases or a single phase with Δ = 1
/// (plain successive shortest paths from a pseudo-flow).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scaling {
    On,
    Off,
}

/// Dual prices proving optimality: one non-negative per-share value for every
/// trade (`u_p` for purchases, `v_s` for sales), indexed like the trade slice.
///
/// Feasibility: `u_p + v_s >= gain(p, s)` for every matchable pair. By weak
/// duality any matching's profit is at most `sum(shares * dual)`, so a matching
/// whose profit equals that bound is optimal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Certificate {
    pub duals: Vec<i64>,
}

impl Certificate {
    pub fn dual_objective(&self, trades: &[Trade]) -> i128 {
        trades.iter().zip(&self.duals).map(|(t, &d)| t.shares as i128 * d as i128).sum()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SolveStats {
    pub pair_arcs: usize,
    pub phases: u32,
    pub augmentations: u64,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Solution {
    pub pairs: Vec<MatchedPair>,
    pub total: i128,
    pub certificate: Certificate,
    pub stats: SolveStats,
}

const SOURCE: usize = 0;
const SINK: usize = 1;

/// Residual network in compressed adjacency form. Arc `2k` is the k-th arc
/// added, arc `2k + 1` its reverse, so `a ^ 1` is always the partner arc.
struct Network {
    start: Vec<usize>,
    adj: Vec<u32>,
    to: Vec<u32>,
    cap: Vec<i64>,
    cost: Vec<i64>,
}

impl Network {
    fn build(nodes: usize, arcs: &[(usize, usize, i64, i64)]) -> Network {
        let m = arcs.len() * 2;
        let mut to = vec![0u32; m];
        let mut cap = vec![0i64; m];
        let mut cost = vec![0i64; m];
        let mut degree = vec![0usize; nodes + 1];
        for (k, &(u, v, c, w)) in arcs.iter().enumerate() {
            to[2 * k] = v as u32;
            to[2 * k + 1] = u as u32;
            cap[2 * k] = c;
            cost[2 * k] = w;
            cost[2 * k + 1] = -w;
            degree[u + 1] += 1;
            degree[v + 1] += 1;
        }
        for i in 0..nodes {
            degree[i + 1] += degree[i];
        }
        let start = degree.clone();
        let mut fill = degree;
        let mut adj = vec![0u32; m];
        for (k, &(u, v, _, _)) in arcs.iter().enumerate() {
            adj[fill[u]] = (2 * k) as u32;
            fill[u] += 1;
            adj[fill[v]] = (2 * k + 1) as u32;
            fill[v] += 1;
        }
        Network { start, adj, to, cap, cost }
    }

    fn arcs_of(&self, u: usize) -> &[u32] {
        &self.adj[self.start[u]..self.start[u + 1]]
    }
}

/// Finds a matching of purchases to sales with the largest total profit.
pub fn solve(trades: &[Trade], rule: &dyn WindowRule, scaling: Scaling) -> Result<Solution, String> {
    validate(trades)?;
    let n = trades.len();
    let nodes = n + 2;
    let day: Vec<i64> = trades.iter().map(|t| t.date.day_number()).collect();
    let end: Vec<i64> = trades.iter().map(|t| rule.window_end(t.date).day_number()).collect();
    let buys: Vec<usize> = (0..n).filter(|&i| trades[i].side == Side::Buy).collect();
    let sells: Vec<usize> = (0..n).filter(|&i| trades[i].side == Side::Sell).collect();

    let mut arcs: Vec<(usize, usize, i64, i64)> = Vec::new();
    let mut buy_total: i64 = 0;
    let mut sell_total: i64 = 0;
    for &p in &buys {
        arcs.push((SOURCE, p + 2, trades[p].shares as i64, 0));
        buy_total += trades[p].shares as i64;
    }
    for &s in &sells {
        arcs.push((s + 2, SINK, trades[s].shares as i64, 0));
        sell_total += trades[s].shares as i64;
    }
    // Capacities one above anything a balanced flow can carry, so these arcs
    // are never saturated at the optimum and the duals below stay feasible.
    arcs.push((SINK, SOURCE, buy_total.min(sell_total) + 1, 0));
    let first_pair_arc = arcs.len();
    let mut pair_ends: Vec<(usize, usize)> = Vec::new();
    for &p in &buys {
        for &s in &sells {
            let in_window = if day[p] <= day[s] { day[s] < end[p] } else { day[p] < end[s] };
            if in_window && trades[s].price > trades[p].price {
                let c = trades[p].shares.min(trades[s].shares) as i64 + 1;
                arcs.push((p + 2, s + 2, c, -(trades[s].price - trades[p].price)));
                pair_ends.push((p, s));
            }
        }
    }

    let mut net = Network::build(nodes, &arcs);
    let original_cap: Vec<i64> = arcs.iter().map(|a| a.2).collect();
    let max_cap = original_cap.iter().copied().max().unwrap_or(1).max(1);
    let mut delta: i64 = match scaling {
        Scaling::On => 1i64 << (63 - max_cap.leading_zeros()),
        Scaling::Off => 1,
    };

    let mut stats = SolveStats { pair_arcs: pair_ends.len(), ..SolveStats::default() };
    let mut excess = vec![0i64; nodes];
    let mut pot = vec![0i64; nodes];
    let mut dist = vec![0i64; nodes];
    let mut pred = vec![u32::MAX; nodes];
    let mut stamp = vec![0u64; nodes];
    let mut settled = vec![0u64; nodes];
    let mut search: u64 = 0;
    let mut heap: BinaryHeap<Reverse<(i64, u32)>> = BinaryHeap::new();
    let mut order: Vec<u32> = Vec::new();

    loop {
        stats.phases += 1;
        // Arcs with less than Δ residual capacity are ignored during a phase and
        // may carry negative reduced cost; saturating them now restores the
        // invariant for the Δ-residual network of the new phase.
        for a in 0..net.to.len() {
            let u = net.to[a ^ 1] as usize;
            let v = net.to[a] as usize;
            if net.cap[a] >= delta && net.cost[a] + pot[u] - pot[v] < 0 {
                let amount = net.cap[a];
                net.cap[a] = 0;
                net.cap[a ^ 1] += amount;
                excess[u] -= amount;
                excess[v] += amount;
            }
        }

        loop {
            search += 1;
            heap.clear();
            order.clear();
            for v in 0..nodes {
                if excess[v] >= delta {
                    stamp[v] = search;
                    dist[v] = 0;
                    pred[v] = u32::MAX;
                    heap.push(Reverse((0, v as u32)));
                }
            }
            if heap.is_empty() {
                break;
            }
            let mut target = None;
            while let Some(Reverse((d, u))) = heap.pop() {
                let u = u as usize;
                if settled[u] == search || d > dist[u] {
                    continue;
                }
                settled[u] = search;
                order.push(u as u32);
                if excess[u] <= -delta {
                    target = Some(u);
                    break;
                }
                for &a in net.arcs_of(u) {
                    let a = a as usize;
                    if net.cap[a] < delta {
                        continue;
                    }
                    let v = net.to[a] as usize;
                    let nd = d + net.cost[a] + pot[u] - pot[v];
                    if stamp[v] != search || nd < dist[v] {
                        stamp[v] = search;
                        dist[v] = nd;
                        pred[v] = a as u32;
                        heap.push(Reverse((nd, v as u32)));
                    }
                }
            }
            let Some(t) = target else { break };

            // Johnson update π(v) += min(d(v), d(t)), shifted by -d(t) for every
            // node; the uniform shift cancels in reduced costs, so only nodes
            // settled before t need touching.
            let bound = dist[t];
            for &v in &order {
                let v = v as usize;
                if dist[v] < bound {
                    pot[v] += dist[v] - bound;
                }
            }

            let mut amount = -excess[t];
            let mut v = t;
            while pred[v] != u32::MAX {
                let a = pred[v] as usize;
                amount = amount.min(net.cap[a]);
                v = net.to[a ^ 1] as usize;
            }
            amount = amount.min(excess[v]);
            let s = v;
            let mut v = t;
            while pred[v] != u32::MAX {
                let a = pred[v] as usize;
                net.cap[a] -= amount;
                net.cap[a ^ 1] += amount;
                v = net.to[a ^ 1] as usize;
            }
            excess[s] -= amount;
            excess[t] += amount;
            stats.augmentations += 1;
        }

        if delta == 1 {
            break;
        }
        delta /= 2;
    }
    assert!(excess.iter().all(|&e| e == 0), "circulation left unbalanced");

    let mut pairs = Vec::new();
    let mut total: i128 = 0;
    for (k, &(p, s)) in pair_ends.iter().enumerate() {
        let arc = 2 * (first_pair_arc + k);
        let flow = original_cap[first_pair_arc + k] - net.cap[arc];
        if flow > 0 {
            let profit = flow as i128 * (trades[s].price - trades[p].price) as i128;
            total += profit;
            pairs.push(MatchedPair { purchase: p, sale: s, shares: flow as u64, profit });
        }
    }

    let duals = (0..n)
        .map(|i| match trades[i].side {
            Side::Buy => (pot[i + 2] - pot[SOURCE]).max(0),
            Side::Sell => (pot[SINK] - pot[i + 2]).max(0),
        })
        .collect();

    Ok(Solution { pairs, total, certificate: Certificate { duals }, stats })
}

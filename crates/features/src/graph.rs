use std::collections::{HashMap, HashSet};

use fraud_common::types::Address;

/// Lightweight in-memory graph tracking address interactions.
///
/// Used to derive graph-based features such as in/out degree, unique
/// counterparties, and fan-in / fan-out patterns.
#[derive(Debug, Default)]
pub struct AddressGraph {
    /// address → set of addresses it sent to
    outgoing: HashMap<Address, HashSet<Address>>,
    /// address → set of addresses it received from
    incoming: HashMap<Address, HashSet<Address>>,
    /// address → total number of outgoing transactions
    out_count: HashMap<Address, u64>,
    /// address → total number of incoming transactions
    in_count: HashMap<Address, u64>,
}

impl AddressGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a transfer from `from` to `to`.
    pub fn record_transfer(&mut self, from: &Address, to: &Address) {
        self.outgoing
            .entry(from.clone())
            .or_default()
            .insert(to.clone());
        self.incoming
            .entry(to.clone())
            .or_default()
            .insert(from.clone());
        *self.out_count.entry(from.clone()).or_insert(0) += 1;
        *self.in_count.entry(to.clone()).or_insert(0) += 1;
    }

    /// Number of unique addresses `addr` has sent to.
    pub fn out_degree(&self, addr: &Address) -> usize {
        self.outgoing.get(addr).map_or(0, |s| s.len())
    }

    /// Number of unique addresses that have sent to `addr`.
    pub fn in_degree(&self, addr: &Address) -> usize {
        self.incoming.get(addr).map_or(0, |s| s.len())
    }

    /// Total outgoing transaction count.
    pub fn out_tx_count(&self, addr: &Address) -> u64 {
        self.out_count.get(addr).copied().unwrap_or(0)
    }

    /// Total incoming transaction count.
    pub fn in_tx_count(&self, addr: &Address) -> u64 {
        self.in_count.get(addr).copied().unwrap_or(0)
    }

    /// Fan-out ratio = out_degree / out_tx_count.
    /// High ratio → spreading funds to many unique addresses.
    pub fn fan_out_ratio(&self, addr: &Address) -> f64 {
        let count = self.out_tx_count(addr) as f64;
        if count == 0.0 {
            return 0.0;
        }
        self.out_degree(addr) as f64 / count
    }

    /// Fan-in ratio = in_degree / in_tx_count.
    /// High ratio → receiving from many unique addresses (potential aggregation).
    pub fn fan_in_ratio(&self, addr: &Address) -> f64 {
        let count = self.in_tx_count(addr) as f64;
        if count == 0.0 {
            return 0.0;
        }
        self.in_degree(addr) as f64 / count
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graph_basics() {
        let mut g = AddressGraph::new();
        let a = "0xAAA".to_string();
        let b = "0xBBB".to_string();
        let c = "0xCCC".to_string();

        g.record_transfer(&a, &b);
        g.record_transfer(&a, &c);
        g.record_transfer(&a, &b); // duplicate edge

        assert_eq!(g.out_degree(&a), 2); // B and C
        assert_eq!(g.out_tx_count(&a), 3);
        assert_eq!(g.in_degree(&b), 1);
        assert_eq!(g.in_tx_count(&b), 2);
    }
}

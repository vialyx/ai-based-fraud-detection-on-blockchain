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

    #[test]
    fn unknown_address_returns_zero() {
        let g = AddressGraph::new();
        let unknown = "0xNOBODY".to_string();
        assert_eq!(g.out_degree(&unknown), 0);
        assert_eq!(g.in_degree(&unknown), 0);
        assert_eq!(g.out_tx_count(&unknown), 0);
        assert_eq!(g.in_tx_count(&unknown), 0);
        assert_eq!(g.fan_out_ratio(&unknown), 0.0);
        assert_eq!(g.fan_in_ratio(&unknown), 0.0);
    }

    #[test]
    fn fan_out_ratio_all_unique() {
        let mut g = AddressGraph::new();
        let a = "0xA".to_string();
        // Send to 3 unique addresses → ratio = 3/3 = 1.0
        g.record_transfer(&a, &"0xB".into());
        g.record_transfer(&a, &"0xC".into());
        g.record_transfer(&a, &"0xD".into());
        assert!((g.fan_out_ratio(&a) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn fan_out_ratio_all_same() {
        let mut g = AddressGraph::new();
        let a = "0xA".to_string();
        let b = "0xB".to_string();
        // Send to same address 5 times → ratio = 1/5 = 0.2
        for _ in 0..5 {
            g.record_transfer(&a, &b);
        }
        assert!((g.fan_out_ratio(&a) - 0.2).abs() < 1e-9);
    }

    #[test]
    fn fan_in_ratio() {
        let mut g = AddressGraph::new();
        let target = "0xTarget".to_string();
        // Receive from 2 unique senders, 4 total tx → ratio = 2/4 = 0.5
        g.record_transfer(&"0xA".into(), &target);
        g.record_transfer(&"0xA".into(), &target);
        g.record_transfer(&"0xB".into(), &target);
        g.record_transfer(&"0xB".into(), &target);
        assert!((g.fan_in_ratio(&target) - 0.5).abs() < 1e-9);
        assert_eq!(g.in_degree(&target), 2);
        assert_eq!(g.in_tx_count(&target), 4);
    }
}

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fraud_common::types::Transaction;
use fraud_features::extractor::FeatureExtractor;
use fraud_features::graph::AddressGraph;
use fraud_features::stats::RollingStats;
use rand::Rng;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn random_tx(rng: &mut impl Rng, idx: u64) -> Transaction {
    Transaction {
        hash: format!("0x{:064x}", rng.gen::<u128>()),
        block_number: 18_000_000 + idx,
        from: format!("0x{:040x}", rng.gen::<u64>()),
        to: Some(format!("0x{:040x}", rng.gen::<u64>())),
        value: rng.gen_range(0..100_000_000_000_000_000_000u128), // 0-100 ETH
        gas_price: Some(rng.gen_range(1_000_000_000..100_000_000_000u128)),
        max_fee_per_gas: Some(rng.gen_range(10_000_000_000..200_000_000_000u128)),
        max_priority_fee_per_gas: Some(rng.gen_range(100_000_000..5_000_000_000u128)),
        gas_used: rng.gen_range(21_000..1_000_000),
        input: (0..rng.gen_range(0..256)).map(|_| rng.gen()).collect(),
        nonce: rng.gen_range(0..10_000),
        tx_index: idx,
        timestamp: 1_700_000_000 + idx,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: RollingStats
// ---------------------------------------------------------------------------

fn bench_rolling_stats_push(c: &mut Criterion) {
    let mut group = c.benchmark_group("rolling_stats_push");
    for window in [64, 256, 1024] {
        group.bench_with_input(
            BenchmarkId::from_parameter(window),
            &window,
            |b, &window| {
                let mut stats = RollingStats::new(window);
                let mut i = 0.0;
                b.iter(|| {
                    i += 1.0;
                    stats.push(black_box(i));
                });
            },
        );
    }
    group.finish();
}

fn bench_rolling_stats_z_score(c: &mut Criterion) {
    let mut group = c.benchmark_group("rolling_stats_z_score");
    for window in [64, 256, 1024] {
        group.bench_with_input(
            BenchmarkId::from_parameter(window),
            &window,
            |b, &window| {
                let mut stats = RollingStats::new(window);
                for i in 0..window {
                    stats.push(i as f64 * 1.1);
                }
                b.iter(|| {
                    black_box(stats.z_score());
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Benchmarks: AddressGraph
// ---------------------------------------------------------------------------

fn bench_address_graph_record(c: &mut Criterion) {
    let mut group = c.benchmark_group("address_graph_record_transfer");
    let mut rng = rand::thread_rng();
    // Pre-generate addresses
    let addrs: Vec<String> = (0..1000).map(|_| format!("0x{:040x}", rng.gen::<u64>())).collect();

    for n_addrs in [100, 500, 1000] {
        group.bench_with_input(
            BenchmarkId::from_parameter(n_addrs),
            &n_addrs,
            |b, &n| {
                let mut graph = AddressGraph::new();
                let mut i = 0usize;
                b.iter(|| {
                    let from = &addrs[i % n];
                    let to = &addrs[(i + 1) % n];
                    graph.record_transfer(from, to);
                    i += 1;
                });
            },
        );
    }
    group.finish();
}

fn bench_address_graph_fan_out(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let hub = "0xHUB".to_string();
    let mut graph = AddressGraph::new();
    for _ in 0..500 {
        let target = format!("0x{:040x}", rng.gen::<u64>());
        graph.record_transfer(&hub, &target);
    }

    c.bench_function("address_graph_fan_out_ratio_500", |b| {
        b.iter(|| {
            black_box(graph.fan_out_ratio(&hub));
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: FeatureExtractor (end-to-end)
// ---------------------------------------------------------------------------

fn bench_feature_extraction(c: &mut Criterion) {
    let mut group = c.benchmark_group("feature_extraction");
    let mut rng = rand::thread_rng();
    let txs: Vec<Transaction> = (0..500).map(|i| random_tx(&mut rng, i)).collect();

    group.bench_function("extract_single_tx", |b| {
        let mut extractor = FeatureExtractor::new();
        let mut idx = 0usize;
        b.iter(|| {
            let tx = &txs[idx % txs.len()];
            black_box(extractor.extract(tx));
            idx += 1;
        });
    });

    group.bench_function("extract_block_200_txs", |b| {
        let block: Vec<&Transaction> = txs.iter().take(200).collect();
        b.iter(|| {
            let mut extractor = FeatureExtractor::new();
            for tx in &block {
                black_box(extractor.extract(tx));
            }
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_rolling_stats_push,
    bench_rolling_stats_z_score,
    bench_address_graph_record,
    bench_address_graph_fan_out,
    bench_feature_extraction,
);
criterion_main!(benches);

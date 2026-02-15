use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fraud_common::config::{IsolationForestConfig, ModelWeights, ScorerConfig};
use fraud_common::types::{Feature, FeatureVector};
use fraud_scorer::ensemble::EnsembleScorer;
use fraud_scorer::rules::RuleEngine;
use rand::Rng;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn random_fv(rng: &mut impl Rng) -> FeatureVector {
    FeatureVector {
        tx_hash: format!("0x{:064x}", rng.gen::<u128>()),
        block_number: 18_000_000,
        features: vec![
            Feature { name: "value_eth".into(), value: rng.gen_range(0.0..500.0) },
            Feature { name: "value_log".into(), value: (rng.gen_range(0.01..500.0_f64) + 1.0).ln() },
            Feature { name: "value_z_score".into(), value: rng.gen_range(-3.0..6.0) },
            Feature { name: "gas_used".into(), value: rng.gen_range(21_000.0..1_000_000.0) },
            Feature { name: "gas_z_score".into(), value: rng.gen_range(-2.0..4.0) },
            Feature { name: "gas_price_gwei".into(), value: rng.gen_range(1.0..200.0) },
            Feature { name: "input_size".into(), value: rng.gen_range(0.0..1000.0) },
            Feature { name: "is_contract_call".into(), value: if rng.gen_bool(0.5) { 1.0 } else { 0.0 } },
            Feature { name: "is_contract_creation".into(), value: if rng.gen_bool(0.05) { 1.0 } else { 0.0 } },
            Feature { name: "sender_out_degree".into(), value: rng.gen_range(0.0..100.0) },
            Feature { name: "sender_out_tx_count".into(), value: rng.gen_range(1.0..500.0) },
            Feature { name: "sender_fan_out_ratio".into(), value: rng.gen_range(0.0..1.0) },
            Feature { name: "receiver_in_degree".into(), value: rng.gen_range(0.0..200.0) },
            Feature { name: "receiver_fan_in_ratio".into(), value: rng.gen_range(0.0..1.0) },
            Feature { name: "nonce".into(), value: rng.gen_range(0.0..5000.0) },
            Feature { name: "is_first_tx".into(), value: if rng.gen_bool(0.1) { 1.0 } else { 0.0 } },
        ],
        extracted_at: Utc::now(),
    }
}

fn test_config() -> ScorerConfig {
    ScorerConfig {
        alert_threshold: 0.65,
        model_weights: ModelWeights {
            isolation_forest: 0.4,
            statistical: 0.35,
            rules: 0.25,
        },
        isolation_forest: IsolationForestConfig {
            num_trees: 100,
            sample_size: 256,
            min_training_samples: 200,
            retrain_interval: 5000,
        },
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: RuleEngine
// ---------------------------------------------------------------------------

fn bench_rule_engine(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let engine = RuleEngine::new();
    let fvs: Vec<FeatureVector> = (0..500).map(|_| random_fv(&mut rng)).collect();

    c.bench_function("rule_engine_evaluate", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let fv = &fvs[idx % fvs.len()];
            black_box(engine.evaluate(fv));
            idx += 1;
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: EnsembleScorer (warm-up phase)
// ---------------------------------------------------------------------------

fn bench_ensemble_warmup(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let fvs: Vec<FeatureVector> = (0..500).map(|_| random_fv(&mut rng)).collect();

    c.bench_function("ensemble_score_warmup", |b| {
        let mut scorer = EnsembleScorer::new(&test_config());
        let mut idx = 0usize;
        b.iter(|| {
            let fv = &fvs[idx % fvs.len()];
            black_box(scorer.score(fv));
            idx += 1;
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: EnsembleScorer (trained phase)
// ---------------------------------------------------------------------------

fn bench_ensemble_trained(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let mut scorer = EnsembleScorer::new(&test_config());

    // Train the model by feeding enough samples
    for _ in 0..300 {
        let fv = random_fv(&mut rng);
        scorer.score(&fv);
    }

    let fvs: Vec<FeatureVector> = (0..500).map(|_| random_fv(&mut rng)).collect();

    c.bench_function("ensemble_score_trained", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let fv = &fvs[idx % fvs.len()];
            black_box(scorer.score(fv));
            idx += 1;
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: Block batch scoring (200 tx)
// ---------------------------------------------------------------------------

fn bench_score_block_batch(c: &mut Criterion) {
    let mut group = c.benchmark_group("block_batch_scoring");
    let mut rng = rand::thread_rng();

    for batch_size in [50, 100, 200] {
        let fvs: Vec<FeatureVector> = (0..batch_size).map(|_| random_fv(&mut rng)).collect();

        group.bench_with_input(
            BenchmarkId::from_parameter(batch_size),
            &fvs,
            |b, fvs| {
                b.iter(|| {
                    let mut scorer = EnsembleScorer::new(&test_config());
                    for fv in fvs {
                        black_box(scorer.score(fv));
                    }
                });
            },
        );
    }
    group.finish();
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

criterion_group!(
    benches,
    bench_rule_engine,
    bench_ensemble_warmup,
    bench_ensemble_trained,
    bench_score_block_batch,
);
criterion_main!(benches);

use chrono::Utc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion};
use fraud_common::config::StorageConfig;
use fraud_common::types::{Alert, FraudScore, ModelScore, RiskLevel};
use fraud_storage::Storage;
use rand::Rng;
use uuid::Uuid;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn temp_storage() -> Storage {
    let dir = std::env::temp_dir().join(format!("bench_storage_{}", rand::thread_rng().gen::<u32>()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = StorageConfig {
        db_path: dir.join("bench.db").to_string_lossy().to_string(),
        enabled: true,
    };
    Storage::new(&config).unwrap()
}

fn random_score(rng: &mut impl Rng) -> FraudScore {
    FraudScore {
        id: Uuid::new_v4(),
        tx_hash: format!("0x{:064x}", rng.gen::<u128>()),
        block_number: rng.gen_range(18_000_000..19_000_000),
        score: rng.gen_range(0.0..1.0),
        risk_level: RiskLevel::Medium,
        model_scores: vec![
            ModelScore { model_name: "isolation_forest".into(), score: rng.gen(), weight: 0.4 },
            ModelScore { model_name: "statistical".into(), score: rng.gen(), weight: 0.35 },
            ModelScore { model_name: "rules".into(), score: rng.gen(), weight: 0.25 },
        ],
        triggered_rules: vec!["high_value_transfer".into()],
        scored_at: Utc::now(),
    }
}

fn random_alert(rng: &mut impl Rng) -> Alert {
    Alert {
        id: Uuid::new_v4(),
        fraud_score: random_score(rng),
        from: format!("0x{:040x}", rng.gen::<u64>()),
        to: Some(format!("0x{:040x}", rng.gen::<u64>())),
        value: rng.gen_range(0..1_000_000_000_000_000_000u128),
        created_at: Utc::now(),
        acknowledged: false,
    }
}

// ---------------------------------------------------------------------------
// Benchmarks: Insert operations
// ---------------------------------------------------------------------------

fn bench_insert_score(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let storage = temp_storage();
    let scores: Vec<FraudScore> = (0..500).map(|_| random_score(&mut rng)).collect();

    c.bench_function("storage_insert_score", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let s = &scores[idx % scores.len()];
            black_box(storage.insert_score(s).unwrap());
            idx += 1;
        });
    });
}

fn bench_insert_alert(c: &mut Criterion) {
    let mut rng = rand::thread_rng();
    let storage = temp_storage();
    let alerts: Vec<Alert> = (0..200).map(|_| random_alert(&mut rng)).collect();

    c.bench_function("storage_insert_alert", |b| {
        let mut idx = 0usize;
        b.iter(|| {
            let a = &alerts[idx % alerts.len()];
            black_box(storage.insert_alert(a).unwrap());
            idx += 1;
        });
    });
}

// ---------------------------------------------------------------------------
// Benchmarks: Query operations
// ---------------------------------------------------------------------------

fn bench_query_recent_scores(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_query_scores");
    let mut rng = rand::thread_rng();
    let storage = temp_storage();

    // Seed database
    for _ in 0..1000 {
        storage.insert_score(&random_score(&mut rng)).unwrap();
    }

    for limit in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(limit),
            &limit,
            |b, &limit| {
                b.iter(|| {
                    black_box(storage.get_recent_scores(limit).unwrap());
                });
            },
        );
    }
    group.finish();
}

fn bench_query_recent_alerts(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_query_alerts");
    let mut rng = rand::thread_rng();
    let storage = temp_storage();

    // Seed database
    for _ in 0..500 {
        storage.insert_alert(&random_alert(&mut rng)).unwrap();
    }

    for limit in [10, 50, 100] {
        group.bench_with_input(
            BenchmarkId::from_parameter(limit),
            &limit,
            |b, &limit| {
                b.iter(|| {
                    black_box(storage.get_recent_alerts(limit).unwrap());
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
    bench_insert_score,
    bench_insert_alert,
    bench_query_recent_scores,
    bench_query_recent_alerts,
);
criterion_main!(benches);

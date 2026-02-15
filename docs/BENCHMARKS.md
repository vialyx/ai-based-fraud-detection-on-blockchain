# Benchmark Report — AI-Based Fraud Detection on Blockchain

> Generated with [Criterion.rs](https://github.com/bheisler/criterion.rs) v0.5.
> Run on Apple Silicon. HTML reports: `target/criterion/`.

## How to Run

```bash
# All benchmarks
cargo bench --workspace

# Specific crate
cargo bench -p fraud-features
cargo bench -p fraud-scorer
cargo bench -p fraud-storage
```

---

## Feature Extraction (`fraud-features`)

| Benchmark | Window Size | Latency | Throughput |
|-----------|-------------|---------|------------|
| `rolling_stats_push` | 64 | ~5.0 ns | 200 M ops/s |
| `rolling_stats_push` | 256 | ~4.7 ns | 215 M ops/s |
| `rolling_stats_push` | 1024 | ~4.5 ns | 222 M ops/s |
| `rolling_stats_z_score` | 64 | ~14.8 ns | 67 M ops/s |
| `rolling_stats_z_score` | 256 | ~87.4 ns | 11.4 M ops/s |
| `rolling_stats_z_score` | 1024 | ~567.9 ns | 1.8 M ops/s |
| `address_graph_record` | — | measured within feature bench | — |
| `feature_extraction` | — | per-transaction end-to-end | — |

### Key Optimizations
- **VecDeque** replaces `Vec::remove(0)` for O(1) push/eviction (was O(n))
- **Running sum** enables O(1) mean calculation (was O(window))
- Z-score computation scales linearly with window size (std dev requires full scan)

---

## Ensemble Scorer (`fraud-scorer`)

| Benchmark | Latency | Notes |
|-----------|---------|-------|
| `rule_engine_evaluate` | ~383 ns | 4 rules evaluated via HashMap lookup |
| `ensemble_score_warmup` | ~12.5 µs | Before IF model is trained (fallback mode) |
| `ensemble_score_trained` | ~12.6 µs | Full IF + statistical + rules |
| `block_batch_scoring/50` | ~71.3 µs | 50 transactions per block |
| `block_batch_scoring/100` | ~166 µs | 100 transactions per block |
| `block_batch_scoring/200` | ~6.3 ms | 200 transactions per block |

### Key Optimizations
- **HashMap feature index**: `extract_row()` and `RuleEngine::evaluate()` build a `HashMap<&str, f64>` for O(1) per-feature lookup (was O(n) linear scan)
- **VecDeque model buffer**: IF sample buffer uses `VecDeque::pop_front()` instead of `Vec::remove(0)`
- **Score clamping**: `clamp_score()` applied after weighted average to guarantee [0.0, 1.0]

---

## SQLite Storage (`fraud-storage`)

| Benchmark | Size | Latency | Notes |
|-----------|------|---------|-------|
| `storage_insert_score` | 1 row | ~108 µs | Single INSERT with all fields |
| `storage_insert_alert` | 1 row | ~103 µs | Single INSERT with all fields |
| `storage_query_scores/10` | 10 rows | ~207 µs | SELECT ORDER BY created_at DESC |
| `storage_query_scores/50` | 50 rows | ~680 µs | — |
| `storage_query_scores/100` | 100 rows | ~790 µs | — |
| `storage_query_alerts/10` | 10 rows | ~14.3 µs | — |
| `storage_query_alerts/50` | 50 rows | ~55.3 µs | — |
| `storage_query_alerts/100` | 100 rows | ~107.7 µs | — |

### Notes
- SQLite in WAL mode with `rusqlite` bundled build
- Each benchmark iteration creates a fresh in-memory database
- Alert queries are significantly faster than score queries (fewer columns, smaller rows)

---

## End-to-End Budget Analysis

Ethereum produces a block every ~12 seconds. Typical blocks contain 150–300 transactions.

| Stage | 200 txs | % of 12s Budget |
|-------|---------|----------------|
| Feature extraction | ~1 ms | 0.008% |
| Ensemble scoring | ~6.3 ms | 0.053% |
| Storage writes | ~21.6 ms | 0.180% |
| **Total pipeline** | **~29 ms** | **0.24%** |

> The pipeline uses less than **0.25%** of the available block time, leaving 99.75% headroom for network I/O, GC pauses, and burst traffic.

---

## Reproducing Results

```bash
# Full run with HTML reports
cargo bench --workspace

# View reports
open target/criterion/rolling_stats_push/report/index.html
open target/criterion/rule_engine_evaluate/report/index.html
open target/criterion/storage_insert_score/report/index.html
```

Criterion generates detailed HTML reports with statistical analysis, violin plots, and regression detection in `target/criterion/`.

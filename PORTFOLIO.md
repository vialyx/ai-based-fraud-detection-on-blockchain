# 📋 Portfolio — AI-Based Fraud Detection on Blockchain

> A capstone project demonstrating systems programming, machine learning,
> blockchain integration, and security engineering — all in Rust.

---

## Project Summary

A **real-time fraud detection pipeline** that monitors every transaction on
Ethereum, extracts 16 behavioral features, scores anomalies with an ensemble
of Isolation Forest + statistical models + heuristic rules, and delivers
alerts through a live dashboard, REST API, WebSocket feed, and Slack webhooks.

**Built entirely in Rust** for maximum performance and type safety.

---

## Key Metrics

```
┌─────────────────────────────────────────────────┐
│                PROJECT METRICS                  │
├────────────────────┬────────────────────────────┤
│  Codebase          │  ~5,500 lines of Rust      │
│  Architecture      │  8 crates + root binary    │
│  Test Coverage     │  124 unit tests (all ✅)    │
│  Benchmarks        │  13 Criterion benchmarks   │
│  ML Model AUC      │  0.9567 ROC AUC            │
│  Best F1           │  0.8571                    │
│  Scoring Latency   │  12.5 µs per transaction   │
│  Block Budget Used │  < 0.25% of 12s            │
│  Features per Tx   │  16 numeric features       │
│  Development Time  │  5 weeks                   │
└────────────────────┴────────────────────────────┘
```

---

## Architecture

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                         FRAUD DETECTION PIPELINE                            │
│                                                                              │
│  ┌──────────┐   ┌──────────────┐   ┌──────────────┐   ┌────────────────┐   │
│  │ Ethereum │──▶│ Block        │──▶│ Feature      │──▶│ Ensemble       │   │
│  │ JSON-RPC │   │ Indexer      │   │ Extractor    │   │ Scorer         │   │
│  │ (Alloy)  │   │              │   │ (16 feat.)   │   │                │   │
│  └──────────┘   └──────────────┘   └──────────────┘   │ ┌────────────┐ │   │
│                                                        │ │ Isolation  │ │   │
│                                                        │ │ Forest     │ │   │
│                                                        │ │ (0.40)     │ │   │
│                                                        │ ├────────────┤ │   │
│                                                        │ │ Statistical│ │   │
│                                                        │ │ Z-Score    │ │   │
│                                                        │ │ (0.35)     │ │   │
│                                                        │ ├────────────┤ │   │
│                                                        │ │ Rule       │ │   │
│                                                        │ │ Engine     │ │   │
│                                                        │ │ (0.25)     │ │   │
│                                                        │ └────────────┘ │   │
│                                                        └───────┬────────┘   │
│                                                                │            │
│                    ┌───────────────────────┬────────────────────┤            │
│                    ▼                       ▼                    ▼            │
│           ┌──────────────┐       ┌──────────────┐     ┌──────────────┐      │
│           │ Alert        │       │ REST API     │     │ SQLite       │      │
│           │ Dispatcher   │       │ + WebSocket  │     │ Storage      │      │
│           │ • stdout     │       │ + Dashboard  │     │ (WAL mode)   │      │
│           │ • Slack      │       │ (Axum v0.8)  │     │              │      │
│           │ • webhook    │       └──────────────┘     └──────────────┘      │
│           └──────────────┘                                                   │
│                                                                              │
│  ┌──────────────────────────────────────────────────────────────────────┐    │
│  │ SECURITY LAYER: Input validation · SSRF protection · NaN guards ·   │    │
│  │ CORS policy · Security headers · Body limits · Mutex poison recovery│    │
│  └──────────────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## Skills Demonstrated

### Systems Programming (Rust)
- Multi-crate Cargo workspace with clean module boundaries
- Async pipeline with `tokio::mpsc` channels and `tokio::spawn`
- Zero-copy data flow through `Arc` and typed enums
- `VecDeque` ring buffers, `HashMap` indexes, running-sum accumulators

### Machine Learning / Data Science
- Extended Isolation Forest (unsupervised anomaly detection)
- 16-dimensional feature engineering with z-scores and graph metrics
- Online learning with automatic model retraining
- ROC AUC, precision/recall/F1 evaluation, threshold sweep

### Blockchain / Web3
- Ethereum JSON-RPC integration via Alloy v1
- Transaction receipt analysis (actual gas consumed)
- Address interaction graph for sybil/wash-trade detection
- Contract creation and calldata pattern recognition

### Security Engineering
- SSRF protection (private IP blocking on webhooks)
- Input validation (hex format, string bounds, score clamping)
- NaN/Infinity guards across the ML pipeline
- API hardening (CORS, headers, body limits, mutex safety)
- Dedicated security module with 15 unit tests

### DevOps / Infrastructure
- Multi-stage Docker build (Rust builder → Debian slim runtime)
- Docker Compose with health checks and volume mounts
- SQLite with WAL mode for concurrent reads
- Criterion benchmarking with HTML reports

### Full-Stack Development
- Axum v0.8 REST API with 5 endpoints + WebSocket
- Dark-themed responsive dashboard (vanilla HTML/JS/CSS)
- Real-time WebSocket alert feed with flash animations
- Auto-refreshing data tables

---

## Tech Stack

| Layer | Technology |
|-------|-----------|
| Language | Rust 2021 edition |
| Async | Tokio |
| Blockchain | Alloy v1 (Ethereum JSON-RPC) |
| ML | Extended Isolation Forest v0.2, ndarray |
| HTTP | Axum v0.8, tower-http |
| Database | SQLite (rusqlite, WAL mode) |
| Benchmarks | Criterion v0.5 |
| Config | TOML |
| Logging | tracing + tracing-subscriber |
| Deployment | Docker, Docker Compose |

---

## Development Timeline

| Week | Focus | Highlights |
|------|-------|-----------|
| 21 | Foundation | 8-crate workspace, pipeline architecture, domain types |
| 22 | ML Core | Real Isolation Forest, receipt-level gas, SQLite persistence |
| 23 | Validation | Historical replay engine, backtesting, ROC AUC = 0.9567 |
| 24 | Frontend | Live dashboard, Docker deployment, README documentation |
| 25 | Hardening | Criterion benchmarks, O(1) optimizations, security audit, portfolio |

---

## Benchmark Highlights

| Operation | Latency |
|-----------|---------|
| Rolling stats push | ~5 ns |
| Rule engine evaluate | ~383 ns |
| Full ensemble score (per tx) | ~12.5 µs |
| Block scoring (200 txs) | ~6.3 ms |
| SQLite insert | ~108 µs |

> The entire pipeline consumes < 0.25% of Ethereum's 12-second block time.

---

## Links

- **Source Code**: [GitHub Repository](https://github.com/example/ai-based-fraud-detection-on-blockchain)
- **Blog Post**: [Medium — Building a Real-Time AI Fraud Detector for Ethereum in Rust](https://medium.com/@your-handle/post-slug)
- **Architecture**: [docs/architecture.md](docs/architecture.md)
- **Benchmarks**: [docs/BENCHMARKS.md](docs/BENCHMARKS.md)
- **License**: MIT

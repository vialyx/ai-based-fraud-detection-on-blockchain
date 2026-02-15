# 🔍 AI-Based Fraud Detection on Blockchain

> A real-time fraud detection pipeline that monitors Ethereum transactions,
> extracts behavioral features, scores anomalies using an ensemble of
> **Isolation Forest + statistical models + heuristic rules**, and delivers
> alerts through a **dashboard**, **REST API**, **WebSocket feed**, and
> optional **Slack webhook**.

[![Rust](https://img.shields.io/badge/Rust-2021-orange?logo=rust)](https://www.rust-lang.org/)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

---

## ✨ Features

| Category | Details |
|----------|---------|
| **Block Indexer** | Polls Ethereum JSON-RPC (Alloy v1) for new blocks, extracts transactions with per-receipt gas |
| **Feature Extraction** | 16 numeric features per tx: value, gas, calldata, address graph, nonce |
| **Ensemble Scoring** | Real Isolation Forest (`extended-isolation-forest`) + statistical z-score model + 4 heuristic rules |
| **Alert Dispatch** | stdout logging, Slack/webhook POST, broadcast channel for WebSocket |
| **REST API** | `/health`, `/api/scores`, `/api/alerts`, `/api/stats`, `/ws` (live alerts) |
| **Dashboard** | Dark-themed HTML/JS/CSS dashboard with live WebSocket feed, auto-refreshing tables |
| **SQLite Persistence** | Fraud scores & alerts stored via `rusqlite` with WAL mode |
| **Historical Replay** | Back-test against labeled CSV datasets with precision/recall/F1/ROC-AUC metrics |
| **Docker** | Multi-stage Dockerfile + Docker Compose for one-command deployment |

---

## 🏗️ Architecture

```
Ethereum RPC ──▶ Block Indexer ──▶ Feature Extractor ──▶ Ensemble Scorer
                                                              │
                         ┌────────────────────────────────────┤
                         ▼                                    ▼
                  Alert Dispatcher                    REST API + WS
                   • log / webhook                   • /dashboard
                   • broadcast ──────────────────────▶  (live UI)
                                                         │
                                                    SQLite Storage
```

See [docs/architecture.md](docs/architecture.md) for full details.

---

## 🚀 Quick Start

### Prerequisites

- **Rust 1.75+** (2021 edition)
- An Ethereum RPC endpoint (default: `https://eth.llamarpc.com`)

### Run Locally

```bash
# Clone
git clone https://github.com/your-user/ai-based-fraud-detection-on-blockchain.git
cd ai-based-fraud-detection-on-blockchain

# Build
cargo build --release

# Run (uses config/default.toml)
cargo run --release

# Dashboard: http://localhost:3000/dashboard/
# API:       http://localhost:3000/health
```

### Run with Docker

```bash
# Build and start
docker compose up -d

# View logs
docker compose logs -f

# Stop
docker compose down
```

---

## ⚙️ Configuration

All settings are in [`config/default.toml`](config/default.toml):

```toml
[pipeline]
channel_buffer = 1024

[indexer]
rpc_url = "https://eth.llamarpc.com"
poll_interval_ms = 12000
# start_block = 19000000          # replay from a specific block

[scorer]
alert_threshold = 0.65

[scorer.model_weights]
isolation_forest = 0.4
statistical = 0.35
rules = 0.25

[scorer.isolation_forest]
num_trees = 100
sample_size = 256
min_training_samples = 512
retrain_interval = 1000

[alerts]
log_enabled = true
# webhook_url = "https://hooks.slack.com/services/XXX"

[api]
bind_address = "127.0.0.1"
port = 3000

[storage]
enabled = true
db_path = "data/fraud.db"
```

---

## 📊 Historical Replay & Back-Testing

Evaluate the pipeline against a labeled dataset:

```bash
cargo run --release --bin backtest -- data/sample_labeled.csv
```

**Options:**

| Flag | Description | Default |
|------|-------------|---------|
| `--threshold <f64>` | Classification threshold | from config |
| `--sweep-steps <n>` | Number of threshold sweep steps | 20 |
| `--config <path>` | Path to config TOML | `config/default.toml` |

**Sample output:**

```
┌──────────────────────────────────────────────┐
│        BACKTEST METRICS REPORT                │
├──────────────────────────────────────────────┤
│  Threshold:       0.6500                     │
│  Precision:       1.0000                     │
│  Recall:          0.1000                     │
│  F1 Score:        0.1818                     │
│  Accuracy:        0.7750                     │
├──────────────────────────────────────────────┤
│  Best F1 Threshold: 0.2000                   │
│  Best F1 Score:     0.8571                   │
│  ROC AUC:           0.9567                   │
└──────────────────────────────────────────────┘
```

### CSV Format

```csv
tx_hash,block_number,from,to,value_wei,gas_used,gas_price_wei,input_hex,nonce,timestamp,is_fraud
0x01,19000001,0xalice,0xbob,1000000000000000000,21000,20000000000,,10,1700000000,0
0xf1,19000001,0xbad,,500000000000000000000,5000000,20000000000,,0,1700000012,1
```

- `to` — leave empty for contract creation
- `input_hex` — hex-encoded calldata (with or without `0x` prefix)
- `is_fraud` — `1` for fraud, `0` for legitimate

---

## 🖥️ Dashboard

The web dashboard is served at **`/dashboard/`** and features:

- **Stats cards** — blocks processed, transactions scored, alerts raised
- **Live alert feed** — real-time WebSocket alerts with flash animation
- **Scores table** — recent fraud scores with visual score bars
- **Alerts table** — persisted alerts with risk badges
- **Auto-refresh** — polls every 5 seconds, WebSocket for instant alerts
- **Dark theme** — modern UI with responsive layout

---

## 🌐 API Reference

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/health` | GET | Health check (`{"status": "ok"}`) |
| `/api/scores` | GET | Recent fraud scores (last 500) |
| `/api/alerts` | GET | Recent alerts (last 500) |
| `/api/stats` | GET | Pipeline counters |
| `/ws` | GET | WebSocket — live alert JSON stream |
| `/dashboard/` | GET | Web dashboard UI |

```bash
# Health check
curl http://localhost:3000/health

# Recent scores
curl http://localhost:3000/api/scores | jq '.scores[:3]'

# Pipeline stats
curl http://localhost:3000/api/stats

# WebSocket (wscat)
wscat -c ws://localhost:3000/ws
```

---

## 🧪 Testing

```bash
# Run all 106 tests
cargo test --workspace

# Specific crate
cargo test -p fraud-replay
cargo test -p fraud-scorer
```

| Crate | Tests |
|-------|-------|
| fraud-common | 13 |
| fraud-features | 18 |
| fraud-scorer | 25 |
| fraud-alerts | 6 |
| fraud-api | 10 |
| fraud-storage | 9 |
| fraud-replay | 25 |
| **Total** | **106** |

---

## 📁 Project Structure

```
├── src/
│   ├── main.rs                # Pipeline orchestrator
│   └── bin/backtest.rs        # Back-test CLI binary
├── crates/
│   ├── common/                # Shared types, config, errors
│   ├── indexer/               # Ethereum RPC block indexer (Alloy)
│   ├── features/              # Feature extraction (16 features/tx)
│   ├── scorer/                # Ensemble scorer (IF + statistical + rules)
│   ├── alerts/                # Alert dispatcher (log, webhook, broadcast)
│   ├── api/                   # Axum REST API + WebSocket + static files
│   ├── storage/               # SQLite persistence (rusqlite)
│   └── replay/                # Historical replay & metrics engine
├── dashboard/                 # Static HTML/JS/CSS dashboard
│   ├── index.html
│   ├── style.css
│   └── app.js
├── config/
│   └── default.toml           # Application configuration
├── data/
│   └── sample_labeled.csv     # Sample back-test dataset (40 txs)
├── docs/
│   └── architecture.md        # Detailed architecture document
├── Dockerfile                 # Multi-stage Docker build
├── docker-compose.yml         # Docker Compose configuration
└── Cargo.toml                 # Workspace manifest
```

---

## Feature Vector (per transaction)

| # | Feature | Description |
|---|---------|-------------|
| 1 | `value_eth` | Transfer value in ETH |
| 2 | `value_log` | Log-scaled value |
| 3 | `value_z_score` | Z-score vs rolling window |
| 4 | `gas_used` | Gas consumed |
| 5 | `gas_z_score` | Gas z-score |
| 6 | `gas_price_gwei` | Gas price in Gwei |
| 7 | `input_size` | Calldata byte length |
| 8 | `is_contract_call` | Has calldata > 4 bytes |
| 9 | `is_contract_creation` | `to` field is null |
| 10 | `sender_out_degree` | Unique recipients of sender |
| 11 | `sender_out_tx_count` | Total outgoing tx count |
| 12 | `sender_fan_out_ratio` | out_degree / out_tx_count |
| 13 | `receiver_in_degree` | Unique senders to receiver |
| 14 | `receiver_fan_in_ratio` | in_degree / in_tx_count |
| 15 | `nonce` | Sender nonce |
| 16 | `is_first_tx` | Nonce == 0 |

---

## 🔧 Tech Stack

| Component | Technology |
|-----------|-----------|
| Language | Rust 2021 edition |
| Async Runtime | Tokio |
| Ethereum RPC | Alloy v1 |
| ML / Anomaly Detection | `extended-isolation-forest` v0.2 |
| Numeric Computing | ndarray |
| HTTP Server | Axum v0.8 |
| Database | SQLite via rusqlite (WAL mode) |
| Serialization | serde + serde_json |
| Configuration | TOML |
| Logging | tracing + tracing-subscriber |
| Containerization | Docker + Docker Compose |

---

## Roadmap

- [x] Week 21 – Project scaffold, pipeline architecture, initial codebase
- [x] Week 22 – Real Isolation Forest (`extended-isolation-forest`), receipt-level gas, SQLite persistence
- [x] Week 23 – Historical replay, back-testing, precision/recall/F1/ROC-AUC metrics
- [x] Week 24 – Dashboard frontend, Docker Compose, README polish

---

## 📝 License

This project is licensed under the [MIT License](LICENSE).

# Architecture – AI-Based Fraud Detection on Blockchain

## High-Level Data Flow

```
┌──────────────┐    ┌───────────────────┐    ┌────────────────┐    ┌───────────────────┐
│   Ethereum   │───▶│  Block Indexer    │───▶│  Feature       │───▶│  Ensemble Scorer  │
│   JSON-RPC   │    │  (fraud-indexer)  │    │  Extractor     │    │  (fraud-scorer)   │
└──────────────┘    └───────────────────┘    │  (fraud-feat.) │    └────────┬──────────┘
                                             └────────────────┘             │
                              ┌─────────────────────────────────────────────┤
                              │                                             │
                    ┌─────────▼──────────┐                     ┌───────────▼──────────┐
                    │  Alert Dispatcher  │                     │    REST API + WS     │
                    │  (fraud-alerts)    │                     │    (fraud-api)       │
                    └─────────┬──────────┘                     └──────────────────────┘
                              │
                    ┌─────────▼──────────┐
                    │  Notification      │
                    │  Channels          │
                    │  • stdout log      │
                    │  • webhook (Slack) │
                    └────────────────────┘
```

## Pipeline Stages

### 1. Block Indexer (`crates/indexer`)
- Connects to an Ethereum RPC endpoint via **Alloy**.
- Polls for new blocks at a configurable interval.
- Converts raw block + transaction data into domain types.
- Emits `PipelineEvent::NewBlock` downstream.

### 2. Feature Extractor (`crates/features`)
- Receives blocks, iterates over every transaction.
- Computes **16 numeric features** per transaction:
  - **Value features**: raw ETH, log-scaled, z-score vs. rolling window.
  - **Gas features**: gas used, gas z-score, gas price in Gwei.
  - **Input features**: calldata size, is contract call, is contract creation.
  - **Graph features**: sender out-degree, fan-out ratio, receiver in-degree, fan-in ratio.
  - **Nonce features**: raw nonce, is first transaction.
- Maintains internal **rolling statistics** (window = 256 txs) and an **address interaction graph**.
- Emits `PipelineEvent::FeaturesExtracted`.

### 3. Ensemble Scorer (`crates/scorer`)
- Three scoring components:
  1. **Isolation-forest proxy** – averages relevant z-scores, sigmoid-squashed.
  2. **Statistical model** – maximum z-score mapped to [0,1].
  3. **Rule engine** – hard-coded heuristic rules (high-value, first-tx, contract creation, high fan-out).
- Weighted combination configurable in `config/default.toml`.
- Risk levels: `Low < 0.40 ≤ Medium < 0.65 ≤ High < 0.85 ≤ Critical`.
- Emits `PipelineEvent::Scored` for every tx, plus `PipelineEvent::AlertRaised` when score ≥ threshold.

### 4. Alert Dispatcher (`crates/alerts`)
- Receives `AlertRaised` events.
- Fans out to all configured **notification channels** (log, webhook).
- Broadcasts JSON to a `tokio::sync::broadcast` channel → consumed by WebSocket clients.

### 5. REST API + WebSocket (`crates/api`)
- **Axum** HTTP server with endpoints:
  | Endpoint          | Method | Description                        |
  |-------------------|--------|------------------------------------|
  | `/health`         | GET    | Health check                       |
  | `/api/scores`     | GET    | Recent fraud scores (ring buffer)  |
  | `/api/alerts`     | GET    | Recent alerts (ring buffer)        |
  | `/api/stats`      | GET    | Pipeline counters                  |
  | `/ws`             | GET    | WebSocket live alert stream        |
  | `/dashboard/*`    | GET    | Static dashboard files             |
- In-memory ring buffers (500 entries each) for scores and alerts.
- Serves the HTML/JS/CSS dashboard from the `dashboard/` directory via `tower-http::services::ServeDir`.

### 6. Historical Replay & Back-Testing (`crates/replay`)
- **Dataset loader** – parses labeled CSV files with 11 columns per transaction.
- **Replay engine** – feeds transactions sequentially through the feature extractor and ensemble scorer, preserving stateful rolling statistics and address graph.
- **Metrics calculator** – computes confusion matrix, precision, recall, F1, accuracy, false-positive rate.
- **Threshold sweep** – evaluates across evenly-spaced thresholds in [0,1], finds best F1, and approximates ROC AUC via the trapezoidal rule.
- Invoked via the `backtest` binary: `cargo run --bin backtest -- data/sample_labeled.csv`.

### 7. SQLite Storage (`crates/storage`)
- Persists fraud scores and alerts to SQLite in WAL mode.
- Schema includes indexed columns for `block_number`, `risk_level`, and `created_at`.
- Provides `insert_score`, `insert_alert`, `get_recent_scores`, `get_recent_alerts`, `count_*` queries.

## Dashboard (`dashboard/`)
A plain HTML + JavaScript + CSS single-page dashboard served at `/dashboard/`:
- **Stats cards** – blocks processed, transactions scored, alerts raised.
- **Live alert feed** – real-time WebSocket alerts with flash animation.
- **Scores table** – recent fraud scores with visual score bars and risk badges.
- **Alerts table** – recent persisted alerts.
- Auto-refreshes REST data every 5 seconds; WebSocket provides instant alert delivery.
- Dark theme, responsive layout, no build step required.

## Deployment
- **Dockerfile** – multi-stage build (Rust builder → Debian slim runtime).
- **docker-compose.yml** – single-service with health check, volume mounts for `data/` and `config/`.

## Shared Types (`crates/common`)
Domain types, configuration structs, and a unified error enum shared across
all crates.

## Configuration
All settings live in `config/default.toml` and are parsed at startup via the
`AppConfig` struct.

## Milestones (Weeks 21–24)

| Week | Deliverable | Status |
|------|-------------|--------|
| 21   | Project scaffold, pipeline architecture, initial codebase | ✅ Done |
| 22   | Real Isolation Forest (`extended-isolation-forest`); receipt-level gas; SQLite persistence | ✅ Done |
| 23   | Historical replay mode; back-test against labeled fraud datasets; precision/recall/F1/ROC-AUC | ✅ Done |
| 24   | Dashboard frontend (HTML+JS+CSS); Docker Compose; README polish | ✅ Done |

# 🔍 AI-Based Fraud Detection on Blockchain

A real-time on-chain fraud detection pipeline written in **Rust**.

Ingests Ethereum blocks → extracts 16 numeric features per transaction → scores
with an ensemble of ML models + heuristic rules → alerts via log / webhook /
WebSocket.

## Quick Start

```bash
# 1. Clone & build
cargo build --release

# 2. Edit the config (default points to a public RPC)
vim config/default.toml

# 3. Run
cargo run --release
# or
./target/release/fraud-detector

# 4. API is at http://127.0.0.1:3000
curl http://127.0.0.1:3000/health
curl http://127.0.0.1:3000/api/stats
```

## WebSocket Live Feed

```bash
websocat ws://127.0.0.1:3000/ws
```

Every alert is pushed as a JSON message in real time.

## Project Structure

```
crates/
├── common/     Shared domain types, config, errors
├── indexer/    Ethereum block ingestion (Alloy RPC)
├── features/   Feature extraction (rolling stats, address graph)
├── scorer/     Ensemble anomaly scoring (ML + rules)
├── alerts/     Alert dispatcher & notification channels
└── api/        REST API + WebSocket (Axum)

src/main.rs     Binary entry point – wires all stages
config/         TOML configuration
docs/           Architecture docs
```

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

## Scoring Engine

Three components combined via weighted average:

1. **Isolation-forest proxy** – z-score–based anomaly score.
2. **Statistical model** – max z-score mapped to [0, 1].
3. **Rule engine** – heuristic flags (high value, first-tx high value, contract creation, high fan-out).

Risk levels: **Low** → **Medium** → **High** → **Critical**.

## Roadmap

- [x] Week 21 – Project scaffold, pipeline architecture, initial codebase
- [ ] Week 22 – Real Isolation Forest (linfa), SQLite persistence
- [ ] Week 23 – Historical replay, back-testing, precision/recall metrics
- [ ] Week 24 – Dashboard frontend, Docker, demo

## License

MIT

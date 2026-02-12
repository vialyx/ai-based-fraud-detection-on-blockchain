use chrono::{DateTime, Utc};
use fraud_common::config::StorageConfig;
use fraud_common::error::Error;
use fraud_common::types::{Alert, FraudScore, RiskLevel};
use rusqlite::{params, Connection};
use std::path::Path;
use std::sync::Mutex;
use tracing::info;

/// SQLite-backed storage for persisting fraud scores and alerts.
pub struct Storage {
    conn: Mutex<Connection>,
}

impl Storage {
    /// Open (or create) the database at the configured path and apply migrations.
    pub fn new(config: &StorageConfig) -> Result<Self, Error> {
        // Ensure the parent directory exists.
        if let Some(parent) = Path::new(&config.db_path).parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                Error::Storage(format!(
                    "cannot create db directory '{}': {e}",
                    parent.display()
                ))
            })?;
        }

        let conn = Connection::open(&config.db_path).map_err(|e| {
            Error::Storage(format!("cannot open database '{}': {e}", config.db_path))
        })?;

        // Enable WAL mode for better concurrent read performance.
        conn.execute_batch("PRAGMA journal_mode=WAL;")
            .map_err(|e| Error::Storage(format!("WAL pragma failed: {e}")))?;

        let storage = Self {
            conn: Mutex::new(conn),
        };
        storage.migrate()?;

        info!(db_path = %config.db_path, "storage initialised");
        Ok(storage)
    }

    /// Create / migrate the schema.
    fn migrate(&self) -> Result<(), Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS scores (
                id            TEXT PRIMARY KEY,
                tx_hash       TEXT NOT NULL,
                block_number  INTEGER NOT NULL,
                score         REAL NOT NULL,
                risk_level    TEXT NOT NULL,
                model_scores  TEXT NOT NULL,     -- JSON array
                triggered_rules TEXT NOT NULL,   -- JSON array
                scored_at     TEXT NOT NULL       -- ISO-8601
            );

            CREATE INDEX IF NOT EXISTS idx_scores_block ON scores(block_number);
            CREATE INDEX IF NOT EXISTS idx_scores_risk  ON scores(risk_level);

            CREATE TABLE IF NOT EXISTS alerts (
                id            TEXT PRIMARY KEY,
                tx_hash       TEXT NOT NULL,
                score         REAL NOT NULL,
                risk_level    TEXT NOT NULL,
                from_addr     TEXT NOT NULL,
                to_addr       TEXT,
                value         TEXT NOT NULL,      -- u128 as text
                created_at    TEXT NOT NULL,       -- ISO-8601
                acknowledged  INTEGER NOT NULL DEFAULT 0
            );

            CREATE INDEX IF NOT EXISTS idx_alerts_created ON alerts(created_at);
            CREATE INDEX IF NOT EXISTS idx_alerts_risk    ON alerts(risk_level);
            ",
        )
        .map_err(|e| Error::Storage(format!("migration failed: {e}")))?;

        Ok(())
    }

    // ── Inserts ──────────────────────────────────────────────────────────

    /// Persist a single fraud score.
    pub fn insert_score(&self, score: &FraudScore) -> Result<(), Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;

        let model_scores_json =
            serde_json::to_string(&score.model_scores).unwrap_or_default();
        let rules_json =
            serde_json::to_string(&score.triggered_rules).unwrap_or_default();
        let risk_str = risk_level_to_str(&score.risk_level);

        conn.execute(
            "INSERT OR REPLACE INTO scores
             (id, tx_hash, block_number, score, risk_level, model_scores, triggered_rules, scored_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                score.id.to_string(),
                score.tx_hash,
                score.block_number as i64,
                score.score,
                risk_str,
                model_scores_json,
                rules_json,
                score.scored_at.to_rfc3339(),
            ],
        )
        .map_err(|e| Error::Storage(format!("insert score failed: {e}")))?;

        Ok(())
    }

    /// Persist a single alert.
    pub fn insert_alert(&self, alert: &Alert) -> Result<(), Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;

        let risk_str = risk_level_to_str(&alert.fraud_score.risk_level);

        conn.execute(
            "INSERT OR REPLACE INTO alerts
             (id, tx_hash, score, risk_level, from_addr, to_addr, value, created_at, acknowledged)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                alert.id.to_string(),
                alert.fraud_score.tx_hash,
                alert.fraud_score.score,
                risk_str,
                alert.from,
                alert.to,
                alert.value.to_string(),
                alert.created_at.to_rfc3339(),
                alert.acknowledged as i32,
            ],
        )
        .map_err(|e| Error::Storage(format!("insert alert failed: {e}")))?;

        Ok(())
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Return the N most recent scores, newest first.
    pub fn get_recent_scores(&self, limit: usize) -> Result<Vec<FraudScore>, Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT id, tx_hash, block_number, score, risk_level,
                        model_scores, triggered_rules, scored_at
                 FROM scores ORDER BY scored_at DESC LIMIT ?1",
            )
            .map_err(|e| Error::Storage(format!("prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let id_str: String = row.get(0)?;
                let tx_hash: String = row.get(1)?;
                let block_number: i64 = row.get(2)?;
                let score: f64 = row.get(3)?;
                let risk_str: String = row.get(4)?;
                let model_scores_json: String = row.get(5)?;
                let rules_json: String = row.get(6)?;
                let scored_at_str: String = row.get(7)?;

                Ok((
                    id_str,
                    tx_hash,
                    block_number,
                    score,
                    risk_str,
                    model_scores_json,
                    rules_json,
                    scored_at_str,
                ))
            })
            .map_err(|e| Error::Storage(format!("query failed: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let (id_str, tx_hash, block_number, score, risk_str, ms_json, rules_json, scored_at_str) =
                row.map_err(|e| Error::Storage(format!("row read failed: {e}")))?;

            let id = uuid::Uuid::parse_str(&id_str)
                .unwrap_or_else(|_| uuid::Uuid::new_v4());
            let risk_level = risk_level_from_str(&risk_str);
            let model_scores = serde_json::from_str(&ms_json).unwrap_or_default();
            let triggered_rules = serde_json::from_str(&rules_json).unwrap_or_default();
            let scored_at: DateTime<Utc> = scored_at_str
                .parse()
                .unwrap_or_else(|_| Utc::now());

            results.push(FraudScore {
                id,
                tx_hash,
                block_number: block_number as u64,
                score,
                risk_level,
                model_scores,
                triggered_rules,
                scored_at,
            });
        }

        Ok(results)
    }

    /// Return the N most recent alerts, newest first.
    pub fn get_recent_alerts(&self, limit: usize) -> Result<Vec<Alert>, Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;

        let mut stmt = conn
            .prepare(
                "SELECT a.id, a.tx_hash, a.score, a.risk_level,
                        a.from_addr, a.to_addr, a.value, a.created_at, a.acknowledged
                 FROM alerts a ORDER BY a.created_at DESC LIMIT ?1",
            )
            .map_err(|e| Error::Storage(format!("prepare failed: {e}")))?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let id_str: String = row.get(0)?;
                let tx_hash: String = row.get(1)?;
                let score: f64 = row.get(2)?;
                let risk_str: String = row.get(3)?;
                let from: String = row.get(4)?;
                let to: Option<String> = row.get(5)?;
                let value_str: String = row.get(6)?;
                let created_at_str: String = row.get(7)?;
                let acknowledged: i32 = row.get(8)?;

                Ok((
                    id_str, tx_hash, score, risk_str, from, to,
                    value_str, created_at_str, acknowledged,
                ))
            })
            .map_err(|e| Error::Storage(format!("query failed: {e}")))?;

        let mut results = Vec::new();
        for row in rows {
            let (id_str, tx_hash, score, risk_str, from, to, value_str, created_at_str, ack) =
                row.map_err(|e| Error::Storage(format!("row read failed: {e}")))?;

            let id = uuid::Uuid::parse_str(&id_str)
                .unwrap_or_else(|_| uuid::Uuid::new_v4());
            let risk_level = risk_level_from_str(&risk_str);
            let value: u128 = value_str.parse().unwrap_or(0);
            let created_at: DateTime<Utc> = created_at_str
                .parse()
                .unwrap_or_else(|_| Utc::now());

            // Reconstruct a minimal FraudScore for the alert.
            let fraud_score = FraudScore {
                id: uuid::Uuid::new_v4(),
                tx_hash: tx_hash.clone(),
                block_number: 0,
                score,
                risk_level,
                model_scores: vec![],
                triggered_rules: vec![],
                scored_at: created_at,
            };

            results.push(Alert {
                id,
                fraud_score,
                from,
                to,
                value,
                created_at,
                acknowledged: ack != 0,
            });
        }

        Ok(results)
    }

    /// Count the total number of stored scores.
    pub fn count_scores(&self) -> Result<u64, Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM scores", [], |row| row.get(0))
            .map_err(|e| Error::Storage(format!("count query failed: {e}")))?;
        Ok(count as u64)
    }

    /// Count the total number of stored alerts.
    pub fn count_alerts(&self) -> Result<u64, Error> {
        let conn = self.conn.lock().map_err(|e| {
            Error::Storage(format!("lock poisoned: {e}"))
        })?;
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM alerts", [], |row| row.get(0))
            .map_err(|e| Error::Storage(format!("count query failed: {e}")))?;
        Ok(count as u64)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn risk_level_to_str(level: &RiskLevel) -> &'static str {
    match level {
        RiskLevel::Low => "low",
        RiskLevel::Medium => "medium",
        RiskLevel::High => "high",
        RiskLevel::Critical => "critical",
    }
}

fn risk_level_from_str(s: &str) -> RiskLevel {
    match s {
        "medium" => RiskLevel::Medium,
        "high" => RiskLevel::High,
        "critical" => RiskLevel::Critical,
        _ => RiskLevel::Low,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fraud_common::types::{FraudScore, ModelScore, RiskLevel};
    use uuid::Uuid;

    fn test_storage() -> Storage {
        let config = StorageConfig {
            db_path: ":memory:".into(),
            enabled: true,
        };
        Storage::new(&config).unwrap()
    }

    #[test]
    fn insert_and_query_scores() {
        let storage = test_storage();

        let score = FraudScore {
            id: Uuid::new_v4(),
            tx_hash: "0xabc".into(),
            block_number: 42,
            score: 0.75,
            risk_level: RiskLevel::High,
            model_scores: vec![ModelScore {
                model_name: "test".into(),
                score: 0.75,
                weight: 1.0,
            }],
            triggered_rules: vec!["high_value".into()],
            scored_at: Utc::now(),
        };

        storage.insert_score(&score).unwrap();

        let recent = storage.get_recent_scores(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].tx_hash, "0xabc");
        assert!((recent[0].score - 0.75).abs() < 1e-9);

        assert_eq!(storage.count_scores().unwrap(), 1);
    }

    #[test]
    fn insert_and_query_alerts() {
        let storage = test_storage();

        let fraud_score = FraudScore {
            id: Uuid::new_v4(),
            tx_hash: "0xdef".into(),
            block_number: 99,
            score: 0.95,
            risk_level: RiskLevel::Critical,
            model_scores: vec![],
            triggered_rules: vec![],
            scored_at: Utc::now(),
        };

        let alert = Alert {
            id: Uuid::new_v4(),
            fraud_score,
            from: "0xsender".into(),
            to: Some("0xreceiver".into()),
            value: 1_000_000,
            created_at: Utc::now(),
            acknowledged: false,
        };

        storage.insert_alert(&alert).unwrap();

        let recent = storage.get_recent_alerts(10).unwrap();
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].from, "0xsender");
        assert!(!recent[0].acknowledged);

        assert_eq!(storage.count_alerts().unwrap(), 1);
    }
}

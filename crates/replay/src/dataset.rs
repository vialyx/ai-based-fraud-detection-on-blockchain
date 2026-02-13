use fraud_common::error::Error;
use fraud_common::types::Transaction;
use serde::Deserialize;
use std::path::Path;

// ---------------------------------------------------------------------------
// CSV row schema
// ---------------------------------------------------------------------------

/// A single row from a labeled fraud dataset CSV.
#[derive(Debug, Clone, Deserialize)]
pub struct CsvRow {
    pub tx_hash: String,
    pub block_number: u64,
    pub from: String,
    #[serde(default)]
    pub to: String,
    pub value_wei: String,
    pub gas_used: u64,
    pub gas_price_wei: String,
    #[serde(default)]
    pub input_hex: String,
    pub nonce: u64,
    pub timestamp: u64,
    pub is_fraud: u8,
}

// ---------------------------------------------------------------------------
// Domain types
// ---------------------------------------------------------------------------

/// A labeled transaction pairing a domain `Transaction` with ground truth.
#[derive(Debug, Clone)]
pub struct LabeledTransaction {
    pub tx: Transaction,
    pub is_fraud: bool,
}

/// Loaded labeled dataset ready for replay.
#[derive(Debug, Clone)]
pub struct Dataset {
    pub entries: Vec<LabeledTransaction>,
}

impl Dataset {
    /// Load a labeled dataset from a CSV file.
    ///
    /// Expected CSV columns:
    /// `tx_hash,block_number,from,to,value_wei,gas_used,gas_price_wei,input_hex,nonce,timestamp,is_fraud`
    ///
    /// - `to` may be empty for contract-creation transactions.
    /// - `input_hex` may be empty for simple ETH transfers.
    /// - `is_fraud` is `1` for fraud and `0` for legitimate.
    pub fn from_csv(path: &Path) -> Result<Self, Error> {
        let mut reader = csv::Reader::from_path(path).map_err(|e| {
            Error::Config(format!(
                "cannot open dataset CSV '{}': {e}",
                path.display()
            ))
        })?;

        let mut entries = Vec::new();
        for (i, result) in reader.deserialize().enumerate() {
            let row: CsvRow = result.map_err(|e| {
                Error::Config(format!("CSV parse error at row {}: {e}", i + 1))
            })?;
            entries.push(row_to_labeled(row, i)?);
        }

        Ok(Dataset { entries })
    }

    /// Build a dataset from an in-memory collection (useful for tests).
    pub fn from_entries(entries: Vec<LabeledTransaction>) -> Self {
        Self { entries }
    }

    /// Total number of transactions in the dataset.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the dataset is empty.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Number of transactions labeled as fraud.
    pub fn fraud_count(&self) -> usize {
        self.entries.iter().filter(|e| e.is_fraud).count()
    }

    /// Number of transactions labeled as legitimate.
    pub fn legit_count(&self) -> usize {
        self.entries.iter().filter(|e| !e.is_fraud).count()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn row_to_labeled(row: CsvRow, index: usize) -> Result<LabeledTransaction, Error> {
    let value: u128 = row
        .value_wei
        .parse()
        .map_err(|e| Error::Config(format!("invalid value_wei at row {}: {e}", index + 1)))?;
    let gas_price: u128 = row
        .gas_price_wei
        .parse()
        .map_err(|e| Error::Config(format!("invalid gas_price_wei at row {}: {e}", index + 1)))?;

    let input = if row.input_hex.is_empty() {
        Vec::new()
    } else {
        hex_decode(&row.input_hex).unwrap_or_default()
    };

    let to = if row.to.is_empty() {
        None
    } else {
        Some(row.to)
    };

    let tx = Transaction {
        hash: row.tx_hash,
        block_number: row.block_number,
        from: row.from,
        to,
        value,
        gas_price: Some(gas_price),
        max_fee_per_gas: Some(gas_price),
        max_priority_fee_per_gas: None,
        gas_used: row.gas_used,
        input,
        nonce: row.nonce,
        tx_index: index as u64,
        timestamp: row.timestamp,
    };

    Ok(LabeledTransaction {
        tx,
        is_fraud: row.is_fraud != 0,
    })
}

/// Decode a hex string (with optional 0x prefix) into bytes.
fn hex_decode(s: &str) -> Option<Vec<u8>> {
    let s = s.strip_prefix("0x").unwrap_or(s);
    if s.is_empty() {
        return Some(Vec::new());
    }
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const HEADER: &str =
        "tx_hash,block_number,from,to,value_wei,gas_used,gas_price_wei,input_hex,nonce,timestamp,is_fraud";

    fn write_csv(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("fraud_replay_test_ds");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(format!("{name}.csv"));
        std::fs::write(&path, content).unwrap();
        path
    }

    #[test]
    fn load_valid_csv() {
        let csv = format!(
            "{HEADER}\n\
             0x01,100,0xfrom,0xto,1000000000000000000,21000,20000000000,,5,1700000000,0\n\
             0x02,101,0xbad,,500000000000000000000,5000000,20000000000,,0,1700000012,1\n\
             0x03,101,0xbob,0xalice,200000000000000000,21000,20000000000,,3,1700000012,0\n"
        );
        let path = write_csv("load_valid", &csv);
        let ds = Dataset::from_csv(&path).unwrap();
        assert_eq!(ds.len(), 3);
        assert_eq!(ds.fraud_count(), 1);
        assert_eq!(ds.legit_count(), 2);
    }

    #[test]
    fn contract_creation_has_none_to() {
        let csv = format!(
            "{HEADER}\n\
             0x01,100,0xfrom,,0,5000000,20000000000,608060405234,0,1700000000,1\n"
        );
        let path = write_csv("contract_creation", &csv);
        let ds = Dataset::from_csv(&path).unwrap();
        assert_eq!(ds.len(), 1);
        assert!(ds.entries[0].tx.to.is_none());
        assert!(ds.entries[0].is_fraud);
    }

    #[test]
    fn missing_file_returns_error() {
        let result = Dataset::from_csv(Path::new("/tmp/nonexistent_dataset_xyz.csv"));
        assert!(result.is_err());
    }

    #[test]
    fn malformed_csv_returns_error() {
        let path = write_csv("malformed", "bad,header\nonly,two");
        let result = Dataset::from_csv(&path);
        assert!(result.is_err());
    }

    #[test]
    fn empty_dataset() {
        let path = write_csv("empty", HEADER);
        let ds = Dataset::from_csv(&path).unwrap();
        assert!(ds.is_empty());
        assert_eq!(ds.fraud_count(), 0);
        assert_eq!(ds.legit_count(), 0);
    }

    #[test]
    fn hex_decode_valid() {
        assert_eq!(hex_decode("a9059cbb"), Some(vec![0xa9, 0x05, 0x9c, 0xbb]));
    }

    #[test]
    fn hex_decode_with_0x_prefix() {
        assert_eq!(
            hex_decode("0xa9059cbb"),
            Some(vec![0xa9, 0x05, 0x9c, 0xbb])
        );
    }

    #[test]
    fn hex_decode_empty() {
        assert_eq!(hex_decode(""), Some(Vec::new()));
        assert_eq!(hex_decode("0x"), Some(Vec::new()));
    }

    #[test]
    fn hex_decode_odd_length() {
        assert_eq!(hex_decode("abc"), None);
    }

    #[test]
    fn input_hex_parsed_as_contract_call() {
        let csv = format!(
            "{HEADER}\n\
             0x01,100,0xfrom,0xto,0,21000,20000000000,0xa9059cbb01020304,5,1700000000,0\n"
        );
        let path = write_csv("input_hex", &csv);
        let ds = Dataset::from_csv(&path).unwrap();
        // 8 bytes of input → is_contract_call should be 1.0 when extracted
        assert_eq!(ds.entries[0].tx.input.len(), 8);
    }
}

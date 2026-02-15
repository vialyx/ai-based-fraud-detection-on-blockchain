//! Security audit module – provides validation, sanitisation, and hardening
//! utilities used across the fraud-detection pipeline.

use std::fmt;

// ---------------------------------------------------------------------------
// Input validation
// ---------------------------------------------------------------------------

/// Maximum allowed length for any Ethereum hex string (addresses, hashes).
pub const MAX_HEX_LEN: usize = 66; // "0x" + 64 hex chars

/// Maximum allowed length for arbitrary string fields persisted to storage.
pub const MAX_STRING_LEN: usize = 256;

/// Maximum allowed length for JSON payloads (webhook bodies, etc.).
pub const MAX_JSON_PAYLOAD: usize = 1_048_576; // 1 MiB

/// Maximum number of WebSocket connections.
pub const MAX_WS_CONNECTIONS: usize = 128;

/// Maximum API response items per page.
pub const MAX_API_PAGE_SIZE: usize = 500;

/// Validation error returned by security checks.
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub field: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "validation error in '{}': {}", self.field, self.message)
    }
}

impl std::error::Error for ValidationError {}

/// Validate that a hex string (address / hash) is well-formed.
pub fn validate_hex_string(field: &str, value: &str) -> Result<(), ValidationError> {
    if value.len() > MAX_HEX_LEN {
        return Err(ValidationError {
            field: field.into(),
            message: format!("exceeds max length {} (got {})", MAX_HEX_LEN, value.len()),
        });
    }
    if !value.starts_with("0x") {
        return Err(ValidationError {
            field: field.into(),
            message: "must start with '0x'".into(),
        });
    }
    if !value[2..].chars().all(|c| c.is_ascii_hexdigit()) {
        return Err(ValidationError {
            field: field.into(),
            message: "contains non-hex characters".into(),
        });
    }
    Ok(())
}

/// Validate that a string does not exceed the maximum allowed length.
pub fn validate_string_length(field: &str, value: &str, max: usize) -> Result<(), ValidationError> {
    if value.len() > max {
        return Err(ValidationError {
            field: field.into(),
            message: format!("exceeds max length {} (got {})", max, value.len()),
        });
    }
    Ok(())
}

/// Validate a URL for webhook endpoints (basic SSRF protection).
pub fn validate_webhook_url(url: &str) -> Result<(), ValidationError> {
    validate_string_length("webhook_url", url, 2048)?;

    // Must be HTTPS in production (allow HTTP for localhost/dev).
    let lower = url.to_lowercase();
    if !lower.starts_with("https://") && !lower.starts_with("http://localhost") && !lower.starts_with("http://127.0.0.1") {
        return Err(ValidationError {
            field: "webhook_url".into(),
            message: "webhook URL must use HTTPS (or localhost for development)".into(),
        });
    }

    // Block private/internal IP ranges (basic SSRF mitigation).
    let blocked_hosts = [
        "//10.", "//172.16.", "//172.17.", "//172.18.", "//172.19.",
        "//172.20.", "//172.21.", "//172.22.", "//172.23.", "//172.24.",
        "//172.25.", "//172.26.", "//172.27.", "//172.28.", "//172.29.",
        "//172.30.", "//172.31.", "//192.168.", "//169.254.",
        "//[::1]", "//0.0.0.0",
    ];

    for blocked in &blocked_hosts {
        if lower.contains(blocked) {
            return Err(ValidationError {
                field: "webhook_url".into(),
                message: "webhook URL must not point to private/internal networks".into(),
            });
        }
    }

    Ok(())
}

/// Validate a score is within [0.0, 1.0].
pub fn validate_score(value: f64) -> Result<(), ValidationError> {
    if !value.is_finite() || value < 0.0 || value > 1.0 {
        return Err(ValidationError {
            field: "score".into(),
            message: format!("score must be in [0.0, 1.0], got {value}"),
        });
    }
    Ok(())
}

/// Validate a port number.
pub fn validate_port(port: u16) -> Result<(), ValidationError> {
    if port == 0 {
        return Err(ValidationError {
            field: "port".into(),
            message: "port must be > 0".into(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Sanitisation
// ---------------------------------------------------------------------------

/// Truncate a string to at most `max_len` **bytes**, appending "..." if truncated.
pub fn sanitize_string(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        // Reserve 3 bytes for the ASCII "..." suffix so result stays ≤ max_len.
        let keep = max_len.saturating_sub(3);
        // Snap to a char boundary to avoid splitting a multi-byte character.
        let end = s.floor_char_boundary(keep);
        format!("{}...", &s[..end])
    }
}

// ---------------------------------------------------------------------------
// NaN / Infinity guards
// ---------------------------------------------------------------------------

/// Replace NaN or Infinity with a safe default.
pub fn safe_f64(value: f64, default: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        default
    }
}

/// Clamp a score to [0.0, 1.0], replacing NaN/Inf with 0.0.
pub fn clamp_score(value: f64) -> f64 {
    safe_f64(value, 0.0).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Integer overflow guards
// ---------------------------------------------------------------------------

/// Safe conversion from u128 to f64 for ETH value representation,
/// guarding against precision loss in extremely large values.
pub fn safe_u128_to_f64(value: u128) -> f64 {
    let result = value as f64;
    if !result.is_finite() {
        f64::MAX
    } else {
        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_hex_address() {
        assert!(validate_hex_string("addr", "0xdead1234abcdef0000000000000000000000cafe").is_ok());
    }

    #[test]
    fn hex_too_long() {
        let long = format!("0x{}", "a".repeat(100));
        assert!(validate_hex_string("hash", &long).is_err());
    }

    #[test]
    fn hex_missing_prefix() {
        assert!(validate_hex_string("addr", "deadbeef").is_err());
    }

    #[test]
    fn hex_invalid_chars() {
        assert!(validate_hex_string("addr", "0xGGGG").is_err());
    }

    #[test]
    fn string_length_ok() {
        assert!(validate_string_length("f", "hello", 10).is_ok());
    }

    #[test]
    fn string_length_exceeded() {
        assert!(validate_string_length("f", &"a".repeat(300), 256).is_err());
    }

    #[test]
    fn webhook_url_https_ok() {
        assert!(validate_webhook_url("https://example.com/hook").is_ok());
    }

    #[test]
    fn webhook_url_localhost_ok() {
        assert!(validate_webhook_url("http://localhost:9000/hook").is_ok());
    }

    #[test]
    fn webhook_url_http_external_rejected() {
        assert!(validate_webhook_url("http://example.com/hook").is_err());
    }

    #[test]
    fn webhook_url_private_ip_rejected() {
        assert!(validate_webhook_url("https://192.168.1.1/hook").is_err());
        assert!(validate_webhook_url("https://10.0.0.1/hook").is_err());
        assert!(validate_webhook_url("https://172.16.0.1/hook").is_err());
    }

    #[test]
    fn valid_scores() {
        assert!(validate_score(0.0).is_ok());
        assert!(validate_score(0.5).is_ok());
        assert!(validate_score(1.0).is_ok());
    }

    #[test]
    fn invalid_scores() {
        assert!(validate_score(-0.1).is_err());
        assert!(validate_score(1.1).is_err());
        assert!(validate_score(f64::NAN).is_err());
        assert!(validate_score(f64::INFINITY).is_err());
    }

    #[test]
    fn port_validation() {
        assert!(validate_port(3000).is_ok());
        assert!(validate_port(0).is_err());
    }

    #[test]
    fn sanitize_within_limit() {
        assert_eq!(sanitize_string("hello", 10), "hello");
    }

    #[test]
    fn sanitize_truncates() {
        let long = "a".repeat(300);
        let result = sanitize_string(&long, 100);
        assert!(result.len() <= 100);
        assert!(result.ends_with("..."));
    }

    #[test]
    fn safe_f64_handles_nan() {
        assert_eq!(safe_f64(f64::NAN, 0.0), 0.0);
        assert_eq!(safe_f64(f64::INFINITY, 0.0), 0.0);
        assert_eq!(safe_f64(42.0, 0.0), 42.0);
    }

    #[test]
    fn clamp_score_bounds() {
        assert_eq!(clamp_score(0.5), 0.5);
        assert_eq!(clamp_score(-0.1), 0.0);
        assert_eq!(clamp_score(1.5), 1.0);
        assert_eq!(clamp_score(f64::NAN), 0.0);
    }

    #[test]
    fn safe_u128_conversion() {
        assert_eq!(safe_u128_to_f64(1_000_000), 1_000_000.0);
        assert!(safe_u128_to_f64(u128::MAX).is_finite());
    }
}

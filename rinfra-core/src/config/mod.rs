mod model;
pub mod watch;

pub use model::*;

use crate::error::{AppError, ErrorCode};

/// Parse a level string into a valid tracing level.
pub fn parse_log_level(level: &str) -> Result<tracing::Level, AppError> {
    level.parse().map_err(|_| {
        AppError::new(
            ErrorCode::LogInitFailed,
            format!("invalid log level: '{level}', expected one of: trace, debug, info, warn, error"),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_log_level_valid() {
        assert_eq!(parse_log_level("trace").unwrap(), tracing::Level::TRACE);
        assert_eq!(parse_log_level("debug").unwrap(), tracing::Level::DEBUG);
        assert_eq!(parse_log_level("info").unwrap(), tracing::Level::INFO);
        assert_eq!(parse_log_level("warn").unwrap(), tracing::Level::WARN);
        assert_eq!(parse_log_level("error").unwrap(), tracing::Level::ERROR);
    }

    #[test]
    fn test_parse_log_level_case_insensitive() {
        assert_eq!(parse_log_level("INFO").unwrap(), tracing::Level::INFO);
        assert_eq!(parse_log_level("Debug").unwrap(), tracing::Level::DEBUG);
    }

    #[test]
    fn test_parse_log_level_invalid() {
        let result = parse_log_level("verbose");
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::LogInitFailed);
    }
}

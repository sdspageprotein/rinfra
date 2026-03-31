mod code;

pub use code::ErrorCode;

use std::fmt;

#[derive(Debug)]
pub struct AppError {
    pub code: ErrorCode,
    pub message: String,
    source: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl AppError {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            source: None,
        }
    }

    pub fn with_source(
        code: ErrorCode,
        message: impl Into<String>,
        source: impl std::error::Error + Send + Sync + 'static,
    ) -> Self {
        Self {
            code,
            message: message.into(),
            source: Some(Box::new(source)),
        }
    }
}

impl fmt::Display for AppError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for AppError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source
            .as_ref()
            .map(|e| e.as_ref() as &(dyn std::error::Error + 'static))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    #[test]
    fn test_app_error_new_sets_code_and_message() {
        let err = AppError::new(ErrorCode::Internal, "something broke");
        assert_eq!(err.code, ErrorCode::Internal);
        assert_eq!(err.message, "something broke");
    }

    #[test]
    fn test_app_error_display_format() {
        let err = AppError::new(ErrorCode::PluginInitFailed, "codec-json failed to start");
        let display = format!("{err}");
        assert_eq!(display, "[PLUGIN_INIT_FAILED] codec-json failed to start");
    }

    #[test]
    fn test_app_error_with_source() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file missing");
        let err = AppError::with_source(ErrorCode::Internal, "io failure", io_err);
        assert!(err.source().is_some());
    }

    #[test]
    fn test_app_error_without_source() {
        let err = AppError::new(ErrorCode::InvalidArgument, "bad input");
        assert!(err.source().is_none());
    }

    #[test]
    fn test_app_error_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<AppError>();
    }
}

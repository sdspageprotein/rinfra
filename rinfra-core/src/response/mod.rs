use serde::{Deserialize, Serialize};

use crate::error::{AppError, ErrorCode};
use crate::i18n::I18n;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiResponse<T: Serialize> {
    pub code: String,
    pub data: Option<T>,
    pub message: String,
}

impl<T: Serialize> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            code: "OK".to_string(),
            data: Some(data),
            message: "ok".to_string(),
        }
    }

    pub fn success_with_message(data: T, message: impl Into<String>) -> Self {
        Self {
            code: "OK".to_string(),
            data: Some(data),
            message: message.into(),
        }
    }
}

impl ApiResponse<()> {
    pub fn error(err: &AppError) -> Self {
        Self {
            code: err.code.as_str().to_string(),
            data: None,
            message: err.message.clone(),
        }
    }

    pub fn from_error_code(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code: code.as_str().to_string(),
            data: None,
            message: message.into(),
        }
    }
}

impl From<&AppError> for ApiResponse<()> {
    fn from(err: &AppError) -> Self {
        Self::error(err)
    }
}

impl ApiResponse<()> {
    /// Build an error response with an i18n-translated message.
    /// Falls back to `err.message` if no translation is found.
    pub fn error_i18n(err: &AppError, i18n: &dyn I18n, locale: &str) -> Self {
        let key = err.code.i18n_key();
        let translated = i18n.t(&key, locale);
        let message = if translated == key {
            err.message.clone()
        } else {
            translated
        };
        Self {
            code: err.code.as_str().to_string(),
            data: None,
            message,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct User {
        name: String,
    }

    #[test]
    fn test_api_response_success() {
        let user = User {
            name: "alice".to_string(),
        };
        let resp = ApiResponse::success(user);
        assert_eq!(resp.code, "OK");
        assert_eq!(resp.message, "ok");
        assert!(resp.data.is_some());
        assert_eq!(resp.data.unwrap().name, "alice");
    }

    #[test]
    fn test_api_response_success_with_message() {
        let resp = ApiResponse::success_with_message(42, "found it");
        assert_eq!(resp.code, "OK");
        assert_eq!(resp.message, "found it");
        assert_eq!(resp.data, Some(42));
    }

    #[test]
    fn test_api_response_error() {
        let err = AppError::new(ErrorCode::PluginNotFound, "codec-xml not installed");
        let resp = ApiResponse::<()>::error(&err);
        assert_eq!(resp.code, "PLUGIN_NOT_FOUND");
        assert!(resp.data.is_none());
        assert_eq!(resp.message, "codec-xml not installed");
    }

    #[test]
    fn test_api_response_from_error_code() {
        let resp = ApiResponse::<()>::from_error_code(ErrorCode::InvalidArgument, "name is empty");
        assert_eq!(resp.code, "INVALID_ARGUMENT");
        assert!(resp.data.is_none());
        assert_eq!(resp.message, "name is empty");
    }

    #[test]
    fn test_api_response_from_app_error_trait() {
        let err = AppError::new(ErrorCode::Internal, "unexpected");
        let resp: ApiResponse<()> = ApiResponse::from(&err);
        assert_eq!(resp.code, "INTERNAL");
    }

    #[test]
    fn test_api_response_success_json_format() {
        let resp = ApiResponse::success("hello");
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["code"], "OK");
        assert_eq!(json["data"], "hello");
        assert_eq!(json["message"], "ok");
    }

    #[test]
    fn test_api_response_error_i18n() {
        struct TestI18n;
        impl crate::i18n::I18n for TestI18n {
            fn t(&self, key: &str, locale: &str) -> String {
                if key == "error.internal" && locale == "zh" {
                    "Internal Server Error".to_string()
                } else {
                    key.to_string()
                }
            }
            fn available_locales(&self) -> Vec<String> {
                vec!["en".into(), "zh".into()]
            }
            fn default_locale(&self) -> &str {
                "en"
            }
        }
        let err = AppError::new(ErrorCode::Internal, "something broke");
        let resp = ApiResponse::<()>::error_i18n(&err, &TestI18n, "zh");
        assert_eq!(resp.message, "Internal Server Error");

        let resp_en = ApiResponse::<()>::error_i18n(&err, &TestI18n, "en");
        assert_eq!(resp_en.message, "something broke");
    }

    #[test]
    fn test_api_response_error_json_format() {
        let err = AppError::new(ErrorCode::PluginInitFailed, "timeout");
        let resp = ApiResponse::<()>::error(&err);
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["code"], "PLUGIN_INIT_FAILED");
        assert!(json["data"].is_null());
        assert_eq!(json["message"], "timeout");
    }
}

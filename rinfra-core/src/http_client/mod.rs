use std::collections::HashMap;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::AppError;

/// HTTP method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Head,
    Options,
}

/// An outgoing HTTP request.
#[derive(Debug, Clone)]
pub struct HttpRequest {
    pub method: HttpMethod,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
    pub timeout_secs: Option<u64>,
}

impl HttpRequest {
    pub fn new(method: HttpMethod, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HashMap::new(),
            body: Vec::new(),
            timeout_secs: None,
        }
    }

    pub fn get(url: impl Into<String>) -> Self {
        Self::new(HttpMethod::Get, url)
    }

    pub fn post(url: impl Into<String>, body: Vec<u8>) -> Self {
        let mut req = Self::new(HttpMethod::Post, url);
        req.body = body;
        req
    }

    pub fn header(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(key.into(), value.into());
        self
    }

    pub fn timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }
}

/// An HTTP response from an external call.
#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    pub fn body_text(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

/// Pluggable HTTP client abstraction for outgoing requests.
///
/// Wraps underlying HTTP libraries (e.g. reqwest) behind a
/// framework-agnostic interface with built-in timeout support.
#[async_trait]
pub trait HttpClient: Send + Sync + 'static {
    /// Execute an HTTP request.
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, AppError>;

    /// Convenience: GET request.
    async fn get(&self, url: &str) -> Result<HttpResponse, AppError> {
        self.request(HttpRequest::get(url)).await
    }

    /// Convenience: POST request with body.
    async fn post(&self, url: &str, body: Vec<u8>) -> Result<HttpResponse, AppError> {
        self.request(HttpRequest::post(url, body)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_http_request_builder() {
        let req = HttpRequest::get("https://example.com")
            .header("Authorization", "Bearer token123")
            .timeout(30);
        assert_eq!(req.method, HttpMethod::Get);
        assert_eq!(req.url, "https://example.com");
        assert_eq!(
            req.headers.get("Authorization").unwrap(),
            "Bearer token123"
        );
        assert_eq!(req.timeout_secs, Some(30));
    }

    #[test]
    fn test_http_request_post() {
        let req = HttpRequest::post("https://api.example.com/data", b"hello".to_vec());
        assert_eq!(req.method, HttpMethod::Post);
        assert_eq!(req.body, b"hello");
    }

    #[test]
    fn test_http_response_success() {
        let resp = HttpResponse {
            status: 200,
            headers: HashMap::new(),
            body: b"ok".to_vec(),
        };
        assert!(resp.is_success());
        assert_eq!(resp.body_text(), "ok");
    }

    #[test]
    fn test_http_response_error() {
        let resp = HttpResponse {
            status: 404,
            headers: HashMap::new(),
            body: b"not found".to_vec(),
        };
        assert!(!resp.is_success());
    }

    #[test]
    fn test_http_method_serde() {
        let method = HttpMethod::Post;
        let json = serde_json::to_string(&method).unwrap();
        assert_eq!(json, "\"POST\"");
        let decoded: HttpMethod = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, HttpMethod::Post);
    }
}

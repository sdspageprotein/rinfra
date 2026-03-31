use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rinfra_core::config::HttpClientConfig;
use rinfra_core::error::{AppError, ErrorCode};
use rinfra_core::http_client::{HttpClient, HttpMethod, HttpRequest, HttpResponse};
use rinfra_core::resilience::CircuitBreaker;

/// HTTP client backed by `reqwest` with configurable timeout, retry, and
/// optional circuit breaker for fail-fast protection.
pub struct ReqwestHttpClient {
    client: reqwest::Client,
    max_retries: u32,
    retry_delay: Duration,
    breaker: Option<Arc<CircuitBreaker>>,
}

impl ReqwestHttpClient {
    pub fn new(config: &HttpClientConfig) -> Result<Self, AppError> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(config.timeout_secs))
            .user_agent(&config.user_agent)
            .build()
            .map_err(|e| {
                AppError::new(
                    ErrorCode::HttpRequestFailed,
                    format!("failed to build HTTP client: {e}"),
                )
            })?;

        Ok(Self {
            client,
            max_retries: config.max_retries,
            retry_delay: Duration::from_millis(config.retry_delay_ms),
            breaker: None,
        })
    }

    pub fn with_circuit_breaker(mut self, breaker: Arc<CircuitBreaker>) -> Self {
        self.breaker = Some(breaker);
        self
    }
}

#[async_trait]
impl HttpClient for ReqwestHttpClient {
    async fn request(&self, req: HttpRequest) -> Result<HttpResponse, AppError> {
        if let Some(ref cb) = self.breaker {
            if !cb.allow_request() {
                return Err(AppError::new(
                    ErrorCode::CircuitBreakerOpen,
                    format!("circuit breaker '{}' is open, rejecting HTTP request to {}", cb.name(), req.url),
                ));
            }
        }

        let mut last_err = None;
        let attempts = self.max_retries + 1;

        for attempt in 0..attempts {
            if attempt > 0 {
                tracing::debug!(
                    attempt,
                    url = %req.url,
                    "retrying HTTP request"
                );
                tokio::time::sleep(self.retry_delay).await;
            }

            match self.do_request(&req).await {
                Ok(resp) => {
                    if let Some(ref cb) = self.breaker {
                        if resp.is_success() {
                            cb.record_success();
                        } else if resp.status >= 500 {
                            cb.record_failure();
                        }
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if let Some(ref cb) = self.breaker {
                        cb.record_failure();
                    }
                    last_err = Some(e);
                }
            }
        }

        Err(last_err.expect("retry loop executed at least once"))
    }
}

impl ReqwestHttpClient {
    async fn do_request(&self, req: &HttpRequest) -> Result<HttpResponse, AppError> {
        let method = match req.method {
            HttpMethod::Get => reqwest::Method::GET,
            HttpMethod::Post => reqwest::Method::POST,
            HttpMethod::Put => reqwest::Method::PUT,
            HttpMethod::Delete => reqwest::Method::DELETE,
            HttpMethod::Patch => reqwest::Method::PATCH,
            HttpMethod::Head => reqwest::Method::HEAD,
            HttpMethod::Options => reqwest::Method::OPTIONS,
        };

        let mut builder = self.client.request(method, &req.url);

        for (k, v) in &req.headers {
            builder = builder.header(k.as_str(), v.as_str());
        }

        if !req.body.is_empty() {
            builder = builder.body(req.body.clone());
        }

        if let Some(secs) = req.timeout_secs {
            builder = builder.timeout(Duration::from_secs(secs));
        }

        let response = builder.send().await.map_err(|e| {
            if e.is_timeout() {
                AppError::new(
                    ErrorCode::HttpTimeout,
                    format!("HTTP request timed out: {}", req.url),
                )
            } else {
                AppError::new(
                    ErrorCode::HttpRequestFailed,
                    format!("HTTP request failed: {e}"),
                )
            }
        })?;

        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();
        let body = response.bytes().await.map_err(|e| {
            AppError::new(
                ErrorCode::HttpRequestFailed,
                format!("failed to read response body: {e}"),
            )
        })?;

        Ok(HttpResponse {
            status,
            headers,
            body: body.to_vec(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> HttpClientConfig {
        HttpClientConfig {
            enabled: true,
            timeout_secs: 10,
            user_agent: "rinfra-test/0.1".into(),
            max_retries: 0,
            retry_delay_ms: 100,
        }
    }

    #[test]
    fn test_client_creation() {
        let client = ReqwestHttpClient::new(&default_config());
        assert!(client.is_ok());
    }

    /// Requires network access.
    #[tokio::test]
    #[ignore]
    async fn test_get_httpbin() {
        let client = ReqwestHttpClient::new(&default_config()).unwrap();
        let resp = client.get("https://httpbin.org/get").await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.is_success());
        assert!(!resp.body.is_empty());
    }

    /// Requires network access.
    #[tokio::test]
    #[ignore]
    async fn test_post_httpbin() {
        let client = ReqwestHttpClient::new(&default_config()).unwrap();
        let resp = client
            .post("https://httpbin.org/post", b"hello".to_vec())
            .await
            .unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.body_text().contains("hello"));
    }

    #[tokio::test]
    async fn test_request_to_invalid_url() {
        let client = ReqwestHttpClient::new(&default_config()).unwrap();
        let result = client.get("http://localhost:1").await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code, ErrorCode::HttpRequestFailed);
    }

    #[tokio::test]
    async fn test_timeout() {
        let mut config = default_config();
        config.timeout_secs = 1;
        let client = ReqwestHttpClient::new(&config).unwrap();
        let req = HttpRequest::get("https://httpbin.org/delay/10").timeout(1);
        let result = client.request(req).await;
        assert!(result.is_err());
    }
}

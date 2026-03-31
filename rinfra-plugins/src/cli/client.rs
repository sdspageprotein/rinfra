use rinfra_core::cli::OutputFormat;
use rinfra_core::config::{ListenerProtocol, RinfraConfig};
use serde::Serialize;

/// HTTP client for CLI commands that talk to a running rinfra instance.
pub struct CliClient {
    base_url: String,
    token: String,
    http: reqwest::Client,
}

#[derive(Debug, Serialize)]
struct ErrorOutput {
    error: String,
}

fn find_http_bind(config: &RinfraConfig) -> String {
    let bind = config
        .plugins
        .net
        .listeners
        .iter()
        .find(|l| l.protocol == ListenerProtocol::Http)
        .map(|l| l.bind.clone())
        .unwrap_or_else(|| "127.0.0.1:8080".to_string());
    if bind.starts_with("0.0.0.0") {
        bind.replacen("0.0.0.0", "127.0.0.1", 1)
    } else {
        bind
    }
}

impl CliClient {
    pub fn for_local(config: &RinfraConfig) -> Self {
        let addr = find_http_bind(config);
        Self {
            base_url: format!("http://{addr}"),
            token: config.plugins.cluster.cluster_token.clone(),
            http: reqwest::Client::new(),
        }
    }

    pub fn for_cluster(config: &RinfraConfig, target_override: Option<&str>) -> Self {
        let addr = if let Some(t) = target_override {
            t.to_string()
        } else if !config.plugins.cluster.main_address.is_empty() {
            config.plugins.cluster.main_address.clone()
        } else {
            find_http_bind(config)
        };

        let base_url = if addr.starts_with("http://") || addr.starts_with("https://") {
            addr
        } else {
            format!("http://{addr}")
        };

        Self {
            base_url,
            token: config.plugins.cluster.cluster_token.clone(),
            http: reqwest::Client::new(),
        }
    }

    // -- Local cache commands --

    pub async fn cache_get(&self, key: &str, format: OutputFormat) {
        let url = format!("{}/api/admin/cache/{key}", self.base_url);
        match self.get_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    pub async fn cache_flush(&self, format: OutputFormat) {
        let url = format!("{}/api/admin/cache/flush", self.base_url);
        match self.post_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    // -- Cluster commands --

    pub async fn cluster_nodes(&self, format: OutputFormat) {
        let url = format!("{}/api/admin/cluster/nodes", self.base_url);
        match self.get_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    pub async fn cluster_info(&self, format: OutputFormat) {
        let url = format!("{}/api/admin/health", self.base_url);
        match self.get_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    pub async fn cluster_cache_get(&self, key: &str, format: OutputFormat) {
        let url = format!("{}/api/admin/cache/{key}", self.base_url);
        match self.get_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    pub async fn cluster_cache_flush(&self, format: OutputFormat) {
        let url = format!("{}/api/admin/cache/flush", self.base_url);
        match self.post_json(&url).await {
            Ok(json) => format.print(&json),
            Err(e) => print_error(&e, format),
        }
    }

    async fn get_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let mut req = self.http.get(url);
        if !self.token.is_empty() {
            req = req.bearer_auth(&self.token);
        }
        let resp = req.send().await.map_err(|e| format!("connection failed: {e}"))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("failed to read response: {e}"))?;

        if !status.is_success() {
            return Err(format!("HTTP {status}: {body}"));
        }

        serde_json::from_str(&body).map_err(|e| format!("invalid JSON response: {e}"))
    }

    async fn post_json(&self, url: &str) -> Result<serde_json::Value, String> {
        let mut req = self.http.post(url);
        if !self.token.is_empty() {
            req = req.bearer_auth(&self.token);
        }
        let resp = req.send().await.map_err(|e| format!("connection failed: {e}"))?;
        let status = resp.status();
        let body = resp
            .text()
            .await
            .map_err(|e| format!("failed to read response: {e}"))?;

        if !status.is_success() {
            return Err(format!("HTTP {status}: {body}"));
        }

        serde_json::from_str(&body).map_err(|e| format!("invalid JSON response: {e}"))
    }
}

fn print_error(msg: &str, format: OutputFormat) {
    let output = ErrorOutput {
        error: msg.to_string(),
    };
    format.print(&output);
    std::process::exit(1);
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::config::{ListenerConfig, ListenerProtocol};

    fn config_with_http_listener(bind: &str) -> RinfraConfig {
        let mut config = RinfraConfig::default();
        config.plugins.net.listeners.push(ListenerConfig {
            name: "main".into(),
            protocol: ListenerProtocol::Http,
            bind: bind.into(),
            http: None,
            tcp: None,
        });
        config
    }

    #[test]
    fn test_cli_client_for_local_normalizes_bind_all() {
        let config = config_with_http_listener("0.0.0.0:8080");
        let client = CliClient::for_local(&config);
        assert_eq!(client.base_url, "http://127.0.0.1:8080");
    }

    #[test]
    fn test_cli_client_for_local_preserves_specific_host() {
        let config = config_with_http_listener("10.0.0.1:9000");
        let client = CliClient::for_local(&config);
        assert_eq!(client.base_url, "http://10.0.0.1:9000");
    }

    #[test]
    fn test_cli_client_for_cluster_uses_main_address() {
        let mut config = RinfraConfig::default();
        config.plugins.cluster.main_address = "192.168.1.10:8089".into();
        let client = CliClient::for_cluster(&config, None);
        assert_eq!(client.base_url, "http://192.168.1.10:8089");
    }

    #[test]
    fn test_cli_client_for_cluster_target_override() {
        let mut config = RinfraConfig::default();
        config.plugins.cluster.main_address = "192.168.1.10:8089".into();
        let client = CliClient::for_cluster(&config, Some("10.0.0.1:7777"));
        assert_eq!(client.base_url, "http://10.0.0.1:7777");
    }

    #[test]
    fn test_cli_client_for_cluster_with_http_prefix() {
        let config = RinfraConfig::default();
        let client = CliClient::for_cluster(&config, Some("http://example.com:8080"));
        assert_eq!(client.base_url, "http://example.com:8080");
    }

    #[test]
    fn test_cli_client_for_cluster_fallback_to_local() {
        let config = config_with_http_listener("0.0.0.0:8089");
        let client = CliClient::for_cluster(&config, None);
        assert_eq!(client.base_url, "http://127.0.0.1:8089");
    }

    #[test]
    fn test_cli_client_carries_token() {
        let mut config = RinfraConfig::default();
        config.plugins.cluster.cluster_token = "secret123".into();
        let client = CliClient::for_cluster(&config, None);
        assert_eq!(client.token, "secret123");
    }
}

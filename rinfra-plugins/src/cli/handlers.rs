use std::path::Path;

use rinfra_core::cli::OutputFormat;
use rinfra_core::config::RinfraConfig;
use serde::Serialize;

use crate::config as config_loader;

const EXAMPLE_CONFIG: &str = include_str!("../../../config/standalone.example.yaml");

#[derive(Debug, Serialize)]
struct ConfigOutput {
    app: AppOutput,
    runtime: RuntimeOutput,
    plugins: PluginsEnabledOutput,
}

#[derive(Debug, Serialize)]
struct AppOutput {
    name: String,
    version: String,
}

#[derive(Debug, Serialize)]
struct RuntimeOutput {
    grace_period_secs: u64,
    component_timeout_secs: u64,
}

#[derive(Debug, Serialize)]
struct PluginsEnabledOutput {
    listeners: Vec<ListenerOutput>,
    cluster_mode: String,
    cluster_role: String,
    enabled: Vec<String>,
}

#[derive(Debug, Serialize)]
struct ListenerOutput {
    name: String,
    protocol: String,
    bind: String,
}

fn collect_enabled(config: &RinfraConfig) -> Vec<String> {
    let mut enabled = Vec::new();
    if config.plugins.log.stdout.enabled {
        enabled.push("log.stdout".into());
    }
    for listener in &config.plugins.net.listeners {
        enabled.push(format!("net.{}", listener.name));
    }
    if config.plugins.store.postgres.enabled {
        enabled.push("store.postgres".into());
    }
    if config.plugins.cache.memory.enabled {
        enabled.push("cache.memory".into());
    }
    if config.plugins.cache.redis.enabled {
        enabled.push("cache.redis".into());
    }
    if config.plugins.cache.multilevel.enabled {
        enabled.push("cache.multilevel".into());
    }
    if config.plugins.ratelimit.memory.enabled {
        enabled.push("ratelimit.memory".into());
    }
    if config.plugins.ratelimit.redis.enabled {
        enabled.push("ratelimit.redis".into());
    }
    if config.plugins.crypto.aesgcm.enabled {
        enabled.push("crypto.aesgcm".into());
    }
    match config.plugins.mq.backend {
        rinfra_core::config::MqBackend::Memory => enabled.push("mq.memory".into()),
        rinfra_core::config::MqBackend::Nats => enabled.push("mq.nats".into()),
        rinfra_core::config::MqBackend::RedisStreams => enabled.push("mq.redis_streams".into()),
        rinfra_core::config::MqBackend::None => {}
    }
    if config.plugins.script.wasm.enabled {
        enabled.push("script.wasm".into());
    }
    if config.plugins.cluster.is_cluster() {
        enabled.push("cluster".into());
    }
    if config.plugins.admin.enabled {
        if config.plugins.admin.auth.enabled {
            enabled.push("admin (auth)".into());
        } else {
            enabled.push("admin".into());
        }
    }
    enabled
}

pub fn handle_config_show(config_path: &str, format: OutputFormat) {
    match load_config(config_path) {
        Ok(config) => {
            let output = ConfigOutput {
                app: AppOutput {
                    name: config.app.name.clone(),
                    version: config.app.version.clone(),
                },
                runtime: RuntimeOutput {
                    grace_period_secs: config.runtime.shutdown.grace_period_secs,
                    component_timeout_secs: config.runtime.shutdown.component_timeout_secs,
                },
                plugins: PluginsEnabledOutput {
                    listeners: config
                        .plugins
                        .net
                        .listeners
                        .iter()
                        .map(|l| ListenerOutput {
                            name: l.name.clone(),
                            protocol: format!("{:?}", l.protocol),
                            bind: l.bind.clone(),
                        })
                        .collect(),
                    cluster_mode: config.plugins.cluster.mode.clone(),
                    cluster_role: config.plugins.cluster.role.clone(),
                    enabled: collect_enabled(&config),
                },
            };
            format.print(&output);
        }
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Serialize)]
struct ValidateResult {
    valid: bool,
    message: String,
    config_path: String,
}

pub fn handle_config_validate(config_path: &str, format: OutputFormat) {
    let result = match load_config(config_path) {
        Ok(_) => ValidateResult {
            valid: true,
            message: "configuration is valid".into(),
            config_path: config_path.into(),
        },
        Err(e) => ValidateResult {
            valid: false,
            message: format!("{e}"),
            config_path: config_path.into(),
        },
    };
    let exit_code = if result.valid { 0 } else { 1 };
    format.print(&result);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

pub fn handle_config_init(output_path: &str, force: bool) {
    let path = Path::new(output_path);
    if path.exists() && !force {
        eprintln!(
            "File '{}' already exists. Use --force to overwrite.",
            output_path
        );
        std::process::exit(1);
    }
    if let Err(e) = std::fs::write(path, EXAMPLE_CONFIG) {
        eprintln!("Failed to write config file: {e}");
        std::process::exit(1);
    }
    println!("Reference configuration written to: {}", output_path);
    println!(
        "Edit the file and save as your config.yaml (or pass --config <path>) to use it."
    );
}

pub fn handle_config_show_defaults(config_path: &str) {
    let config = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };
    match serde_yaml::to_string(&config) {
        Ok(yaml) => print!("{yaml}"),
        Err(e) => {
            eprintln!("Error serializing config: {e}");
            std::process::exit(1);
        }
    }
}

#[derive(Debug, Serialize)]
struct PluginInfo {
    name: String,
    category: String,
    status: String,
}

pub fn handle_plugin_list(config_path: &str, format: OutputFormat) {
    let config = match load_config(config_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Error loading config: {e}");
            std::process::exit(1);
        }
    };

    let mut all = vec![
        ("log.stdout", "logging", config.plugins.log.stdout.enabled),
        (
            "store.postgres",
            "storage",
            config.plugins.store.postgres.enabled,
        ),
        (
            "cache.memory",
            "cache",
            config.plugins.cache.memory.enabled,
        ),
        ("cache.redis", "cache", config.plugins.cache.redis.enabled),
        (
            "cache.multilevel",
            "cache",
            config.plugins.cache.multilevel.enabled,
        ),
        (
            "ratelimit.memory",
            "ratelimit",
            config.plugins.ratelimit.memory.enabled,
        ),
        (
            "ratelimit.redis",
            "ratelimit",
            config.plugins.ratelimit.redis.enabled,
        ),
        (
            "crypto.aesgcm",
            "crypto",
            config.plugins.crypto.aesgcm.enabled,
        ),
        ("codec.json", "codec", true),
        ("codec.msgpack", "codec", true),
        ("codec.protobuf", "codec", true),
        (
            "mq.memory",
            "messaging",
            config.plugins.mq.backend == rinfra_core::config::MqBackend::Memory,
        ),
        (
            "script.wasm",
            "scripting",
            config.plugins.script.wasm.enabled,
        ),
        (
            "cluster",
            "cluster",
            config.plugins.cluster.is_cluster(),
        ),
        ("admin", "admin", config.plugins.admin.enabled),
    ];

    for listener in &config.plugins.net.listeners {
        all.push((&listener.name, "network", true));
    }

    let plugins: Vec<PluginInfo> = all
        .into_iter()
        .map(|(name, category, enabled)| PluginInfo {
            name: name.into(),
            category: category.into(),
            status: if enabled { "enabled" } else { "available" }.into(),
        })
        .collect();

    format.print(&plugins);
}

pub fn load_config(config_path: &str) -> Result<RinfraConfig, rinfra_core::error::AppError> {
    let p = Path::new(config_path);
    let mut cfg = if p.exists() {
        config_loader::load_with_env(p)?
    } else {
        let mut cfg = RinfraConfig::default();
        config_loader::apply_env_overrides(&mut cfg);
        cfg
    };
    cfg.config_path = config_path.to_string();
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_enabled_defaults() {
        let config = RinfraConfig::default();
        let enabled = collect_enabled(&config);
        assert!(enabled.contains(&"log.stdout".to_string()));
        assert!(enabled.contains(&"cache.memory".to_string()));
        assert!(enabled.contains(&"mq.memory".to_string()));
        assert!(enabled.contains(&"admin".to_string()));
        assert!(!enabled.contains(&"store.postgres".to_string()));
    }

    #[test]
    fn test_load_config_missing_file_uses_defaults() {
        let config = load_config("/nonexistent/path.yaml").unwrap();
        assert_eq!(config.app.name, "rinfra-app");
    }

    #[test]
    fn test_config_output_serializable() {
        let output = ConfigOutput {
            app: AppOutput {
                name: "test".into(),
                version: "0.1.0".into(),
            },
            runtime: RuntimeOutput {
                grace_period_secs: 30,
                component_timeout_secs: 10,
            },
            plugins: PluginsEnabledOutput {
                listeners: vec![ListenerOutput {
                    name: "main".into(),
                    protocol: "Http".into(),
                    bind: "0.0.0.0:8080".into(),
                }],
                cluster_mode: "standalone".into(),
                cluster_role: "worker".into(),
                enabled: vec!["net.main".into()],
            },
        };
        let json = serde_json::to_string(&output).unwrap();
        assert!(json.contains("test"));
    }
}

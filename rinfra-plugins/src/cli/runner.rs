use std::path::Path;
use std::sync::Arc;

use axum::Router;
use clap::Parser;
use rinfra_core::appstate::AppState;
use rinfra_core::cli::OutputFormat;
use rinfra_core::config::RinfraConfig;
use rinfra_core::net::tcp::TcpHandler;
use rinfra_core::net::ws::WsHandler;
use tonic::transport::server::Router as TonicRouter;

use super::client::CliClient;
use super::handlers;
use super::{
    CacheCommands, Cli, ClusterCacheCommands, ClusterCommands, Commands, ConfigCommands,
    PluginCommands, ServerArgs, ServerCommands,
};
use crate::Application;
use rinfra_core::plugin::Plugin;

type HttpRouterFn = Box<dyn FnOnce(Arc<AppState>) -> Router + Send>;
type GrpcServiceFn = Box<dyn FnOnce(TonicRouter) -> TonicRouter + Send>;
type ExtraCmdHandler = Box<dyn FnOnce(&[String], &RinfraConfig, OutputFormat) -> bool + Send>;

/// Options for `run()`. Built via the builder pattern.
///
/// # Example (minimal)
/// ```ignore
/// rinfra_plugins::run(RunOptions::new()
///     .http_router("main", |state| my_routes(state))
/// ).await;
/// ```
///
/// # Example (with custom commands)
/// ```ignore
/// rinfra_plugins::run(RunOptions::new()
///     .http_router("main", |state| my_routes(state))
///     .extra_commands(|args, config, format| {
///         match args[0].as_str() {
///             "migrate" => { /* handle */ true }
///             _ => false,
///         }
///     })
/// ).await;
/// ```
pub struct RunOptions {
    plugins: Vec<Box<dyn Plugin>>,
    http_routers: Vec<(String, HttpRouterFn)>,
    ws_handlers: Vec<(String, Arc<dyn WsHandler>)>,
    tcp_handlers: Vec<(String, Arc<dyn TcpHandler>)>,
    grpc_services: Vec<(String, GrpcServiceFn)>,
    extra_cmd_handler: Option<ExtraCmdHandler>,
    node_metadata: std::collections::HashMap<String, String>,
}

impl RunOptions {
    pub fn new() -> Self {
        Self {
            plugins: Vec::new(),
            http_routers: Vec::new(),
            ws_handlers: Vec::new(),
            tcp_handlers: Vec::new(),
            grpc_services: Vec::new(),
            extra_cmd_handler: None,
            node_metadata: std::collections::HashMap::new(),
        }
    }

    pub fn plugin(mut self, plugin: Box<dyn Plugin>) -> Self {
        self.plugins.push(plugin);
        self
    }

    pub fn http_router(
        mut self,
        listener: &str,
        f: impl FnOnce(Arc<AppState>) -> Router + Send + 'static,
    ) -> Self {
        self.http_routers.push((listener.to_string(), Box::new(f)));
        self
    }

    pub fn ws_handler(mut self, listener: &str, handler: Arc<dyn WsHandler>) -> Self {
        self.ws_handlers.push((listener.to_string(), handler));
        self
    }

    pub fn tcp_handler(mut self, listener: &str, handler: Arc<dyn TcpHandler>) -> Self {
        self.tcp_handlers.push((listener.to_string(), handler));
        self
    }

    pub fn grpc_service(
        mut self,
        listener: &str,
        f: impl FnOnce(TonicRouter) -> TonicRouter + Send + 'static,
    ) -> Self {
        self.grpc_services.push((listener.to_string(), Box::new(f)));
        self
    }

    pub fn metadata(mut self, pairs: Vec<(&str, &str)>) -> Self {
        self.node_metadata = pairs
            .into_iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        self
    }

    pub fn extra_commands(
        mut self,
        f: impl FnOnce(&[String], &RinfraConfig, OutputFormat) -> bool + Send + 'static,
    ) -> Self {
        self.extra_cmd_handler = Some(Box::new(f));
        self
    }
}

impl Default for RunOptions {
    fn default() -> Self {
        Self::new()
    }
}

/// One-shot entry point that handles CLI parsing, config loading,
/// and dispatching to built-in or custom commands.
///
/// ```ignore
/// #[tokio::main]
/// async fn main() {
///     rinfra_plugins::run(RunOptions::new()
///         .http_router("main", |state| my_routes(state))
///     ).await;
/// }
/// ```
pub async fn run(opts: RunOptions) {
    let cli = Cli::parse();
    run_with_cli(cli, opts).await;
}

pub async fn run_with_cli(cli: Cli, opts: RunOptions) {
    let config_path = find_config(&cli.config);
    let format: OutputFormat = cli.format.into();

    match cli.command {
        None
        | Some(Commands::Server(ServerArgs {
            command: ServerCommands::Start,
        })) => {
            run_server(&config_path, opts).await;
        }
        Some(Commands::Config(args)) => match args.command {
            ConfigCommands::Show(show_args) => {
                if show_args.defaults {
                    handlers::handle_config_show_defaults(&config_path);
                } else {
                    handlers::handle_config_show(&config_path, format);
                }
            }
            ConfigCommands::Validate => handlers::handle_config_validate(&config_path, format),
            ConfigCommands::Init(init_args) => {
                handlers::handle_config_init(&init_args.output, init_args.force);
            }
        },
        Some(Commands::Plugin(args)) => match args.command {
            PluginCommands::List => handlers::handle_plugin_list(&config_path, format),
        },
        Some(Commands::Cache(args)) => {
            let config = load_or_exit(&config_path);
            let client = CliClient::for_local(&config);
            match args.command {
                CacheCommands::Get { key } => client.cache_get(&key, format).await,
                CacheCommands::Flush => client.cache_flush(format).await,
            }
        }
        Some(Commands::Cluster(args)) => {
            let config = load_or_exit(&config_path);
            let client = CliClient::for_cluster(&config, args.target.as_deref());
            match args.command {
                ClusterCommands::Nodes => client.cluster_nodes(format).await,
                ClusterCommands::Info => client.cluster_info(format).await,
                ClusterCommands::Cache(cache_args) => match cache_args.command {
                    ClusterCacheCommands::Get { key } => {
                        client.cluster_cache_get(&key, format).await;
                    }
                    ClusterCacheCommands::Flush { yes } => {
                        if !yes {
                            eprintln!("Warning: this will flush cache on all cluster nodes.");
                            eprintln!("Use --yes to confirm.");
                            std::process::exit(1);
                        }
                        client.cluster_cache_flush(format).await;
                    }
                },
            }
        }
        Some(Commands::External(args)) => {
            if args.is_empty() {
                eprintln!("Error: empty external subcommand");
                std::process::exit(1);
            }
            let config = load_or_exit(&config_path);
            if let Some(handler) = opts.extra_cmd_handler {
                if !handler(&args, &config, format) {
                    eprintln!("Error: unknown command '{}'", args[0]);
                    eprintln!("Run with --help for available commands.");
                    std::process::exit(1);
                }
            } else {
                eprintln!("Error: unknown command '{}'", args[0]);
                eprintln!("Run with --help for available commands.");
                std::process::exit(1);
            }
        }
    }
}

fn load_or_exit(config_path: &str) -> RinfraConfig {
    handlers::load_config(config_path).unwrap_or_else(|e| {
        eprintln!("Error loading config: {e}");
        std::process::exit(1);
    })
}

fn find_config(hint: &str) -> String {
    if Path::new(hint).exists() {
        return hint.to_string();
    }
    let candidates = [
        "config/standalone.example.yaml",
        "../config/standalone.example.yaml",
        "../../config/standalone.example.yaml",
    ];
    for c in &candidates {
        if Path::new(c).exists() {
            return c.to_string();
        }
    }
    hint.to_string()
}

async fn run_server(config_path: &str, opts: RunOptions) {
    let mut builder = Application::builder().config_path(config_path);

    for p in opts.plugins {
        builder = builder.plugin(p);
    }
    for (listener, f) in opts.http_routers {
        builder = builder.http_router(&listener, f);
    }
    for (listener, h) in opts.ws_handlers {
        builder = builder.ws_handler(&listener, h);
    }
    for (listener, h) in opts.tcp_handlers {
        builder = builder.tcp_handler(&listener, h);
    }
    for (listener, f) in opts.grpc_services {
        builder = builder.grpc_service(&listener, f);
    }
    if !opts.node_metadata.is_empty() {
        builder = builder.node_metadata(opts.node_metadata);
    }

    let app = builder.build().expect("failed to build application");

    if let Err(e) = app.run().await {
        eprintln!("Application error: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_config_prefers_hint_when_it_exists() {
        let result = find_config("Cargo.toml");
        assert_eq!(result, "Cargo.toml");
    }

    #[test]
    fn test_run_options_default() {
        let opts = RunOptions::new();
        assert!(opts.http_routers.is_empty());
        assert!(opts.extra_cmd_handler.is_none());
    }

    #[test]
    fn test_run_options_with_http_router() {
        let opts = RunOptions::new().http_router("main", |_state| Router::new());
        assert_eq!(opts.http_routers.len(), 1);
        assert_eq!(opts.http_routers[0].0, "main");
    }

    #[test]
    fn test_run_options_with_extra_commands() {
        let opts = RunOptions::new().extra_commands(|_args, _config, _format| true);
        assert!(opts.extra_cmd_handler.is_some());
    }

    #[test]
    fn test_run_options_default_trait() {
        let opts = RunOptions::default();
        assert!(opts.http_routers.is_empty());
    }
}

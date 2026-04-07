use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use rinfra_core::appstate::AppState;
use rinfra_core::cluster::NodeRegistry;
use rinfra_core::config::{
    ClusterPluginConfigs, ListenerConfig, ListenerProtocol, RinfraConfig,
};
use rinfra_core::error::AppError;
use rinfra_core::net::tcp::TcpHandler;
use rinfra_core::net::transform::{ByteTransform, CompressorTransform, TransformRegistry};
use rinfra_core::net::ws::WsHandler;
use rinfra_core::plugin::{Plugin, PluginContext, PluginRegistry};
use tokio_util::sync::CancellationToken;
#[cfg(feature = "grpc")]
use tonic::transport::server::Router as TonicRouter;
use tracing::{error, info, warn};

use crate::cluster::{ClusterConnection, ClusterServer, ConnectedRegistry};
use crate::config as config_loader;
use crate::log::init_observability;
use crate::net::middleware::builtin_http_middlewares;
use crate::net::{HttpServer, TcpServer, WsTracker, ws_upgrade_handler};
use crate::plugin::builtin_plugins;
#[cfg(feature = "grpc")]
use crate::rpc::GrpcServer;
use crate::rpc::trpc::handler::TrpcHandler;
use crate::telemetry::{self, OtelGuard};

/// Shared cluster node list, periodically synced from the main node.
/// Available in `AppState` on worker nodes via `state.get::<ClusterNodeList>()`.
#[derive(Clone)]
pub struct ClusterNodeList(pub Arc<tokio::sync::RwLock<Vec<rinfra_core::cluster::NodeInfo>>>);

type HttpRouterFn = Box<dyn FnOnce(Arc<AppState>) -> Router + Send>;
#[cfg(feature = "grpc")]
type GrpcServiceFn = Box<
    dyn FnOnce(TonicRouter) -> TonicRouter + Send,
>;
type TrpcServiceEntry = (
    String,
    Arc<
        dyn Fn(Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, AppError>> + Send>>
            + Send
            + Sync,
    >,
);

pub struct Application {
    config: RinfraConfig,
    registry: PluginRegistry,
    plugins: Vec<Box<dyn Plugin>>,
    http_routers: HashMap<String, Vec<HttpRouterFn>>,
    ws_handlers: HashMap<String, Arc<dyn WsHandler>>,
    tcp_handlers: HashMap<String, Arc<dyn TcpHandler>>,
    #[cfg(feature = "grpc")]
    grpc_services: HashMap<String, Vec<GrpcServiceFn>>,
    trpc_services: HashMap<String, Vec<TrpcServiceEntry>>,
    node_metadata: HashMap<String, String>,
    cluster_registry: Option<Arc<ConnectedRegistry>>,
    cluster_node_list: Option<Arc<tokio::sync::RwLock<Vec<rinfra_core::cluster::NodeInfo>>>>,
    transform_registry: TransformRegistry,
    extra_http_middlewares: Vec<Arc<dyn rinfra_core::net::middleware::HttpMiddleware>>,
    extra_tcp_middlewares: Vec<Arc<dyn rinfra_core::net::tcp::TcpMiddleware>>,
    #[cfg(feature = "metrics")]
    metrics_handle: Option<metrics_exporter_prometheus::PrometheusHandle>,
    otel_guard: Option<OtelGuard>,
}

pub struct ApplicationBuilder {
    config_path: Option<String>,
    config: Option<RinfraConfig>,
    extra_plugins: Vec<Box<dyn Plugin>>,
    http_routers: HashMap<String, Vec<HttpRouterFn>>,
    ws_handlers: HashMap<String, Arc<dyn WsHandler>>,
    tcp_handlers: HashMap<String, Arc<dyn TcpHandler>>,
    #[cfg(feature = "grpc")]
    grpc_services: HashMap<String, Vec<GrpcServiceFn>>,
    trpc_services: HashMap<String, Vec<TrpcServiceEntry>>,
    node_metadata: HashMap<String, String>,
    extra_transforms: Vec<Arc<dyn ByteTransform>>,
    extra_http_middlewares: Vec<Arc<dyn rinfra_core::net::middleware::HttpMiddleware>>,
    extra_tcp_middlewares: Vec<Arc<dyn rinfra_core::net::tcp::TcpMiddleware>>,
}

impl Application {
    pub fn builder() -> ApplicationBuilder {
        ApplicationBuilder {
            config_path: None,
            config: None,
            extra_plugins: Vec::new(),
            http_routers: HashMap::new(),
            ws_handlers: HashMap::new(),
            tcp_handlers: HashMap::new(),
            #[cfg(feature = "grpc")]
            grpc_services: HashMap::new(),
            trpc_services: HashMap::new(),
            node_metadata: HashMap::new(),
            extra_transforms: Vec::new(),
            extra_http_middlewares: Vec::new(),
            extra_tcp_middlewares: Vec::new(),
        }
    }

    pub fn config(&self) -> &RinfraConfig {
        &self.config
    }

    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    async fn build_with_plugins(&mut self) -> Result<AppState, AppError> {
        let mut ctx = PluginContext::new(self.config.clone());
        self.registry.build_all(&self.plugins, &mut ctx).await?;
        let (state, routers) = ctx.into_app_parts();

        for router_any in routers {
            if let Ok(router) = router_any.downcast::<Router>() {
                let router = *router;
                self.http_routers
                    .entry("__plugin__".to_string())
                    .or_default()
                    .push(Box::new(move |_| router));
            }
        }

        Ok(state)
    }

    pub async fn run(mut self) -> Result<(), AppError> {
        let cluster_config = self.config.plugins.cluster.clone();

        info!(
            app = %self.config.app.name,
            version = %self.config.app.version,
            cluster_mode = %cluster_config.mode,
            listeners = self.config.plugins.net.listeners.len(),
            "application starting"
        );

        let token = create_shutdown_token();
        let ws_tracker = Arc::new(WsTracker::new());

        // Cluster setup
        if cluster_config.is_cluster() {
            if cluster_config.is_main() {
                self.setup_main_cluster(&cluster_config, &token).await?;
            } else {
                self.setup_worker_cluster(&cluster_config, &token).await?;
            }
        }

        let mut state = self.build_with_plugins().await?;

        if let Some(reg) = self.cluster_registry.take() {
            state.set::<Arc<dyn NodeRegistry>>(reg as Arc<dyn NodeRegistry>);
        }

        if let Some(node_list) = self.cluster_node_list.take() {
            state.set(ClusterNodeList(node_list));
        }

        #[cfg(feature = "metrics")]
        if let Some(handle) = self.metrics_handle.clone() {
            state.set(handle);
        }

        let state = Arc::new(state);
        let shutdown_config = self.config.runtime.shutdown.clone();
        let listeners = self.config.plugins.net.listeners.clone();
        let metrics_config = self.config.plugins.metrics.clone();
        let health_enabled = self.config.plugins.health.enabled;
        let admin_config = self.config.plugins.admin.clone();
        let telemetry_enabled = self.config.plugins.telemetry.enabled;

        // Build built-in TCP middlewares from AppState (e.g., audit).
        if let Some(audit) = state.audit_logger() {
            self.extra_tcp_middlewares.push(Arc::new(
                crate::net::AuditTcpMiddleware::new(audit.clone()),
            ));
        }

        // Start all listeners
        for listener in &listeners {
            match listener.protocol {
                ListenerProtocol::Http => {
                    self.start_http_listener(
                        listener,
                        &state,
                        &token,
                        &ws_tracker,
                        &metrics_config,
                        health_enabled,
                        &admin_config,
                        telemetry_enabled,
                    );
                }
                ListenerProtocol::Tcp => {
                    self.start_tcp_listener(listener, &token);
                }
                ListenerProtocol::Grpc => {
                    #[cfg(feature = "grpc")]
                    self.start_grpc_listener(listener, &state, &token);
                    #[cfg(not(feature = "grpc"))]
                    error!(listener = %listener.name, "grpc listener requested but 'grpc' feature is not enabled");
                }
                ListenerProtocol::Trpc => {
                    self.start_trpc_listener(listener, &token);
                }
            }
        }

        // Wait for shutdown
        token.cancelled().await;

        drain_ws_connections(&ws_tracker, &shutdown_config).await;
        graceful_shutdown(&mut self.registry, &shutdown_config).await;

        if let Some(guard) = self.otel_guard.take() {
            info!("flushing opentelemetry traces");
            guard.shutdown();
        }

        Ok(())
    }

    async fn setup_main_cluster(
        &mut self,
        config: &ClusterPluginConfigs,
        token: &CancellationToken,
    ) -> Result<(), AppError> {
        info!(role = "main", "running in cluster mode as main node");

        let registry = Arc::new(ConnectedRegistry::new());
        self.cluster_registry = Some(registry.clone());

        let server = ClusterServer::new(
            registry,
            config.cluster_token.clone(),
            config.ping_interval_secs,
        );
        let addr = config.main_address.clone();
        let cancel = token.clone();
        tokio::spawn(async move {
            if let Err(e) = server.run(&addr, cancel).await {
                error!(error = %e, "cluster TCP server failed");
            }
        });

        Ok(())
    }

    async fn setup_worker_cluster(
        &mut self,
        config: &ClusterPluginConfigs,
        token: &CancellationToken,
    ) -> Result<(), AppError> {
        let hostname = hostname::get()
            .ok()
            .and_then(|h| h.into_string().ok())
            .unwrap_or_default();
        let endpoints: Vec<rinfra_core::cluster::Endpoint> = self
            .config
            .plugins
            .net
            .listeners
            .iter()
            .map(|l| {
                let address = if !hostname.is_empty() {
                    l.bind
                        .replace("0.0.0.0", &hostname)
                        .replace("127.0.0.1", &hostname)
                } else {
                    l.bind.clone()
                };
                rinfra_core::cluster::Endpoint {
                    protocol: format!("{:?}", l.protocol).to_lowercase(),
                    address,
                }
            })
            .collect();

        let conn = ClusterConnection::new(config, endpoints, self.node_metadata.clone());
        let node_id = conn.node_id().to_string();
        self.cluster_node_list = Some(conn.node_list());

        info!(
            role = "worker",
            node_id = %node_id,
            main = %config.main_address,
            "running in cluster mode as worker node"
        );

        conn.connect_once().await?;
        let _handle = conn.spawn_background(token.clone());

        Ok(())
    }

    fn start_http_listener(
        &mut self,
        listener: &ListenerConfig,
        state: &Arc<AppState>,
        token: &CancellationToken,
        ws_tracker: &Arc<WsTracker>,
        metrics_config: &rinfra_core::config::MetricsConfig,
        health_enabled: bool,
        _admin_config: &rinfra_core::config::AdminPluginConfigs,
        telemetry_enabled: bool,
    ) {
        let http_opts = listener.http.clone().unwrap_or_default();

        // Build middleware registry from config
        let mut mw_registry = rinfra_core::net::middleware::HttpMiddlewareRegistry::new();
        let builtin_mws = builtin_http_middlewares(
            &http_opts.middleware,
            metrics_config,
            telemetry_enabled,
            state,
        );
        for mw in builtin_mws {
            if let Err(e) = mw_registry.register(mw) {
                warn!(error = %e, "failed to register builtin http middleware");
            }
        }
        for mw in &self.extra_http_middlewares {
            if let Err(e) = mw_registry.register(mw.clone()) {
                warn!(error = %e, "failed to register custom http middleware");
            }
        }

        if !mw_registry.is_empty() {
            let mw_names: Vec<&str> = mw_registry.sorted().iter().map(|m| m.name()).collect();
            info!(
                listener = %listener.name,
                middleware = ?mw_names,
                "http middleware pipeline"
            );
        }

        let mut server = HttpServer::new(listener.bind.clone())
            .with_middleware_registry(mw_registry);

        let router_fns = self.http_routers.remove(&listener.name);
        let plugin_routers = self.http_routers.remove("__plugin__");
        let state_clone = state.clone();

        let mut merged_router: Option<Router> = None;

        if let Some(fns) = plugin_routers {
            for f in fns {
                let r = f(state_clone.clone());
                merged_router = Some(match merged_router {
                    Some(existing) => existing.merge(r),
                    None => r,
                });
            }
        }

        if let Some(fns) = router_fns {
            for f in fns {
                let r = f(state_clone.clone());
                merged_router = Some(match merged_router {
                    Some(existing) => existing.merge(r),
                    None => r,
                });
            }
        }

        if health_enabled {
            let health_router = crate::health::health_router(state_clone.clone());
            merged_router = Some(match merged_router {
                Some(existing) => existing.merge(health_router),
                None => health_router,
            });
        }


        let ws_handler = self.ws_handlers.remove(&listener.name);
        if let Some(ws_h) = ws_handler {
            if http_opts.ws.enabled {
                let ws_config = http_opts.ws.clone();
                let ws_router = Router::new()
                    .route("/ws", axum::routing::get(ws_upgrade_handler).with_state(ws_h))
                    .layer(axum::extract::Extension(token.clone()))
                    .layer(axum::extract::Extension(ws_config))
                    .layer(axum::extract::Extension(ws_tracker.clone()));

                merged_router = Some(match merged_router {
                    Some(existing) => existing.merge(ws_router),
                    None => ws_router,
                });
            } else {
                warn!(
                    listener = %listener.name,
                    "ws handler registered but ws.enabled=false, ignoring"
                );
            }
        }

        if let Some(router) = merged_router {
            server = server.merge_router(router);
        }

        let cancel = token.clone();
        let name = listener.name.clone();
        tokio::spawn(async move {
            info!(listener = %name, protocol = "http", "listener started");
            if let Err(e) = server.start(cancel).await {
                error!(listener = %name, error = %e, "http listener failed");
            }
        });
    }

    fn start_tcp_listener(&mut self, listener: &ListenerConfig, token: &CancellationToken) {
        let handler = match self.tcp_handlers.remove(&listener.name) {
            Some(h) => h,
            None => {
                error!(listener = %listener.name, "tcp listener has no handler, skipping");
                return;
            }
        };

        let tcp_opts = listener.tcp.clone().unwrap_or_default();

        let mut pipeline: Vec<Arc<dyn ByteTransform>> = Vec::new();
        for step in &tcp_opts.pipeline {
            match self.transform_registry.get(&step.transform) {
                Some(t) => pipeline.push(t.clone()),
                None => {
                    error!(
                        listener = %listener.name,
                        transform = %step.transform,
                        "pipeline: transform not found, skipping listener"
                    );
                    return;
                }
            }
        }

        let middlewares = self.extra_tcp_middlewares.clone();

        let server = TcpServer::new(
            listener.bind.clone(),
            listener.name.clone(),
            handler,
        )
        .with_max_frame_size(tcp_opts.max_frame_size)
        .with_pipeline(pipeline)
        .with_middlewares(middlewares);

        let cancel = token.clone();
        tokio::spawn(async move {
            if let Err(e) = server.start(cancel).await {
                error!(error = %e, "tcp listener failed");
            }
        });
    }

    #[cfg(feature = "grpc")]
    fn start_grpc_listener(
        &mut self,
        listener: &ListenerConfig,
        _state: &Arc<AppState>,
        token: &CancellationToken,
    ) {
        let grpc = GrpcServer::new(listener.bind.clone());
        let service_fns = self.grpc_services.remove(&listener.name);
        let notify = grpc.shutdown_handle();
        let cancel = token.clone();

        tokio::spawn(async move {
            let result = if let Some(fns) = service_fns {
                grpc.start_with_routes(|mut router| {
                    for f in fns {
                        router = f(router);
                    }
                    router
                })
                .await
            } else {
                grpc.start_with_routes(|r| r).await
            };
            if let Err(e) = result {
                error!(error = %e, "grpc listener failed");
            }
        });

        tokio::spawn(async move {
            cancel.cancelled().await;
            notify.notify_one();
        });
    }

    fn start_trpc_listener(&mut self, listener: &ListenerConfig, token: &CancellationToken) {
        let entries = self.trpc_services.remove(&listener.name).unwrap_or_default();
        if entries.is_empty() {
            warn!(listener = %listener.name, "trpc listener has no services");
        }

        let handler = Arc::new(TrpcHandler::new());
        for (name, svc_fn) in entries {
            handler.register_raw(&name, svc_fn);
        }

        let tcp_opts = listener.tcp.clone().unwrap_or_default();
        let server = TcpServer::new(
            listener.bind.clone(),
            listener.name.clone(),
            handler,
        )
        .with_max_frame_size(tcp_opts.max_frame_size);

        let cancel = token.clone();
        tokio::spawn(async move {
            if let Err(e) = server.start(cancel).await {
                error!(error = %e, "trpc listener failed");
            }
        });
    }
}

impl ApplicationBuilder {
    pub fn config_path(mut self, path: impl Into<String>) -> Self {
        self.config_path = Some(path.into());
        self
    }

    pub fn config(mut self, config: RinfraConfig) -> Self {
        self.config = Some(config);
        self
    }

    pub fn plugin(mut self, plugin: Box<dyn Plugin>) -> Self {
        self.extra_plugins.push(plugin);
        self
    }

    pub fn http_router(
        mut self,
        listener: &str,
        f: impl FnOnce(Arc<AppState>) -> Router + Send + 'static,
    ) -> Self {
        self.http_routers
            .entry(listener.to_string())
            .or_default()
            .push(Box::new(f));
        self
    }

    pub fn ws_handler(mut self, listener: &str, handler: Arc<dyn WsHandler>) -> Self {
        self.ws_handlers.insert(listener.to_string(), handler);
        self
    }

    pub fn tcp_handler(mut self, listener: &str, handler: Arc<dyn TcpHandler>) -> Self {
        self.tcp_handlers.insert(listener.to_string(), handler);
        self
    }

    #[cfg(feature = "grpc")]
    pub fn grpc_service(
        mut self,
        listener: &str,
        f: impl FnOnce(TonicRouter) -> TonicRouter + Send + 'static,
    ) -> Self {
        self.grpc_services
            .entry(listener.to_string())
            .or_default()
            .push(Box::new(f));
        self
    }

    pub fn trpc_service<F, Fut>(mut self, listener: &str, name: &str, handler: F) -> Self
    where
        F: Fn(Vec<u8>) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<Vec<u8>, AppError>> + Send + 'static,
    {
        let wrapped: Arc<
            dyn Fn(Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, AppError>> + Send>>
                + Send
                + Sync,
        > = Arc::new(move |payload| Box::pin(handler(payload)));

        self.trpc_services
            .entry(listener.to_string())
            .or_default()
            .push((name.to_string(), wrapped));
        self
    }

    pub fn http_middleware(
        mut self,
        mw: Arc<dyn rinfra_core::net::middleware::HttpMiddleware>,
    ) -> Self {
        self.extra_http_middlewares.push(mw);
        self
    }

    pub fn tcp_middleware(
        mut self,
        mw: Arc<dyn rinfra_core::net::tcp::TcpMiddleware>,
    ) -> Self {
        self.extra_tcp_middlewares.push(mw);
        self
    }

    pub fn byte_transform(mut self, transform: Arc<dyn ByteTransform>) -> Self {
        self.extra_transforms.push(transform);
        self
    }

    pub fn node_metadata(mut self, metadata: HashMap<String, String>) -> Self {
        self.node_metadata = metadata;
        self
    }

    pub fn build(self) -> Result<Application, AppError> {
        let config = if let Some(config) = self.config {
            config
        } else if let Some(path) = &self.config_path {
            let p = Path::new(path);
            if p.exists() {
                config_loader::load_with_env(p)?
            } else {
                info!(path = %path, "config file not found, using defaults");
                let mut cfg = RinfraConfig::default();
                config_loader::apply_env_overrides(&mut cfg);
                cfg
            }
        } else {
            let mut cfg = RinfraConfig::default();
            config_loader::apply_env_overrides(&mut cfg);
            cfg
        };

        let telemetry_init = telemetry::init_telemetry(
            &config.plugins.telemetry,
            &config.app.name,
        )?;

        init_observability(
            &config.plugins.log,
            #[cfg(feature = "telemetry")]
            telemetry_init.otel_layer,
        )?;

        let otel_guard = telemetry_init.guard;

        let mut plugins = builtin_plugins();
        for p in self.extra_plugins {
            plugins.push(p);
        }

        let mut transform_registry = TransformRegistry::new();
        for c in crate::compress::builtin_compressors() {
            transform_registry.register(Arc::new(CompressorTransform(c)))?;
        }
        for t in self.extra_transforms {
            transform_registry.register(t)?;
        }
        let transform_names = transform_registry.names();
        if !transform_names.is_empty() {
            info!(transforms = ?transform_names, "byte transforms registered");
        }

        #[cfg(feature = "metrics")]
        let metrics_handle = if config.plugins.metrics.enabled {
            match crate::metrics::init_metrics() {
                Ok(handle) => Some(handle),
                Err(e) => {
                    error!(error = %e, "failed to init prometheus metrics");
                    None
                }
            }
        } else {
            None
        };

        Ok(Application {
            config,
            registry: PluginRegistry::new(),
            plugins,
            http_routers: self.http_routers,
            ws_handlers: self.ws_handlers,
            tcp_handlers: self.tcp_handlers,
            #[cfg(feature = "grpc")]
            grpc_services: self.grpc_services,
            trpc_services: self.trpc_services,
            node_metadata: self.node_metadata,
            cluster_registry: None,
            cluster_node_list: None,
            transform_registry,
            extra_http_middlewares: self.extra_http_middlewares,
            extra_tcp_middlewares: self.extra_tcp_middlewares,
            #[cfg(feature = "metrics")]
            metrics_handle,
            otel_guard,
        })
    }
}

fn create_shutdown_token() -> CancellationToken {
    let token = CancellationToken::new();
    let t = token.clone();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        info!("shutdown signal received");
        t.cancel();
    });
    token
}

async fn wait_for_shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();

    #[cfg(unix)]
    {
        let mut sigterm =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .expect("failed to install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = sigterm.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        ctrl_c.await.ok();
    }
}

async fn drain_ws_connections(
    tracker: &Arc<WsTracker>,
    shutdown_config: &rinfra_core::config::ShutdownConfig,
) {
    let count = tracker.active_count();
    if count > 0 {
        info!(active = count, "draining websocket connections");
        let timeout = Duration::from_secs(shutdown_config.grace_period_secs);
        tracker.wait_all_closed(timeout).await;
        info!(remaining = tracker.active_count(), "websocket drain complete");
    }
}

async fn graceful_shutdown(
    registry: &mut PluginRegistry,
    shutdown_config: &rinfra_core::config::ShutdownConfig,
) {
    info!("starting graceful shutdown");
    let component_timeout = Duration::from_secs(shutdown_config.component_timeout_secs);
    if let Err(e) = registry.shutdown_all(component_timeout).await {
        error!(error = %e, "error during plugin shutdown");
    }
    info!("graceful shutdown complete");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_application_builder_default_config() {
        let app = Application::builder().build().unwrap();
        assert_eq!(app.config().app.name, "rinfra-app");
    }

    #[test]
    fn test_application_builder_custom_config() {
        let mut config = RinfraConfig::default();
        config.app.name = "test-app".to_string();
        let app = Application::builder().config(config).build().unwrap();
        assert_eq!(app.config().app.name, "test-app");
    }

    #[test]
    fn test_application_builder_missing_file_uses_defaults() {
        let app = Application::builder()
            .config_path("nonexistent.yaml")
            .build()
            .unwrap();
        assert_eq!(app.config().app.name, "rinfra-app");
    }

    #[test]
    fn test_cluster_config_standalone_default() {
        let config = RinfraConfig::default();
        assert_eq!(config.plugins.cluster.mode, "standalone");
        assert!(!config.plugins.cluster.is_cluster());
    }

    #[tokio::test]
    async fn test_build_with_plugins_cache_enabled() {
        let mut config = RinfraConfig::default();
        config.plugins.cache.memory.enabled = true;
        let mut app = Application::builder().config(config).build().unwrap();
        let state = app.build_with_plugins().await.unwrap();
        assert!(state.cache().is_some());
    }

    #[tokio::test]
    async fn test_build_with_plugins_cache_disabled() {
        let mut config = RinfraConfig::default();
        config.plugins.cache.memory.enabled = false;
        let mut app = Application::builder().config(config).build().unwrap();
        let state = app.build_with_plugins().await.unwrap();
        assert!(state.cache().is_none());
    }

    #[test]
    fn test_builtin_plugins_registered() {
        let app = Application::builder().build().unwrap();
        assert!(!app.plugins.is_empty());
    }

    #[test]
    fn test_builder_http_router() {
        let app = Application::builder()
            .http_router("main", |_state| Router::new())
            .build()
            .unwrap();
        assert!(app.http_routers.contains_key("main"));
    }
}

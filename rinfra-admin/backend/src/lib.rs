pub mod auth;
mod extensions;
mod routes;

use std::path::Path;
use std::sync::Arc;

use axum::Router;
use rinfra_core::appstate::AppState;
use tower_http::services::{ServeDir, ServeFile};

pub use auth::{AdminAuthLayer, AdminRole, AdminTokenInfo, AdminTokenRecord, AdminTokenStore};
pub use extensions::MenuEntry;

/// Backward-compatible entry point. Equivalent to `AdminBuilder::new(static_dir).build(state)`.
pub fn admin_router(static_dir: &str, state: Arc<AppState>) -> Router {
    AdminBuilder::new(static_dir).build(state)
}

/// Builder for constructing an extensible admin router.
///
/// # Example
/// ```ignore
/// use rinfra_admin::{AdminBuilder, MenuEntry};
///
/// AdminBuilder::new("frontend/dist")
///     .route(my_game_admin_routes())
///     .menu(MenuEntry {
///         path: "/game/status".into(),
///         name: "Game Status".into(),
///         icon: "◈".into(),
///         category: "game".into(),
///         data_url: "/api/admin/game/status".into(),
///     })
///     .build(state)
/// ```
pub struct AdminBuilder {
    static_dir: String,
    extra_routes: Vec<Router<Arc<AppState>>>,
    menu_entries: Vec<MenuEntry>,
    token_store: Option<Arc<AdminTokenStore>>,
    auth_exclude_paths: Vec<String>,
}

impl AdminBuilder {
    pub fn new(static_dir: &str) -> Self {
        Self {
            static_dir: static_dir.to_string(),
            extra_routes: Vec::new(),
            menu_entries: Vec::new(),
            token_store: None,
            auth_exclude_paths: Vec::new(),
        }
    }

    /// Inject business-defined admin API routes.
    pub fn route(mut self, router: Router<Arc<AppState>>) -> Self {
        self.extra_routes.push(router);
        self
    }

    /// Declare a menu entry for the admin frontend sidebar.
    pub fn menu(mut self, entry: MenuEntry) -> Self {
        self.menu_entries.push(entry);
        self
    }

    /// Enable admin token authentication with a pre-bootstrapped store.
    pub fn with_auth(
        mut self,
        store: Arc<AdminTokenStore>,
        exclude_paths: Vec<String>,
    ) -> Self {
        self.token_store = Some(store);
        self.auth_exclude_paths = exclude_paths;
        self
    }

    /// Enable admin auth from config — bootstraps the token store automatically.
    /// Generates a root token on first run (printed to stderr).
    pub fn with_auth_config(
        self,
        config: &rinfra_core::config::AdminAuthConfig,
    ) -> Self {
        if !config.enabled {
            return self;
        }
        match auth::bootstrap_admin_auth(config) {
            Ok(store) => self.with_auth(store, config.exclude_paths.clone()),
            Err(e) => {
                tracing::error!(error = %e, "admin auth bootstrap failed — running without auth");
                self
            }
        }
    }

    /// Build the final admin `Router`.
    pub fn build(self, state: Arc<AppState>) -> Router {
        let mut api = routes::admin_routes();

        if let Some(ref store) = self.token_store {
            let audit = state.audit_logger().cloned();
            let token_routes = routes::token_management_routes(store.clone(), audit);
            api = api.merge(token_routes);
        }

        for extra in self.extra_routes {
            api = api.merge(extra);
        }

        let ext_count = self.menu_entries.len();
        let ext_router = extensions::extensions_route(self.menu_entries);

        let mut router = Router::new()
            .nest("/api/admin", api.with_state(state.clone()))
            .merge(Router::new().nest("/api/admin", ext_router));

        if let Some(store) = self.token_store {
            let mut auth_layer = AdminAuthLayer::new(store, self.auth_exclude_paths);
            if let Some(audit) = state.audit_logger() {
                auth_layer = auth_layer.with_audit(audit.clone());
            }
            router = router.layer(auth_layer);
        }

        let dir = Path::new(&self.static_dir);
        if dir.is_dir() {
            tracing::info!(path = %dir.display(), "serving admin frontend from disk");
            let index = dir.join("index.html");
            let serve = ServeDir::new(dir).fallback(ServeFile::new(index));
            router = router.nest_service("/admin", serve);
        } else {
            tracing::warn!(path = %dir.display(), "admin frontend directory not found, API-only mode");
        }

        if ext_count > 0 {
            tracing::info!(count = ext_count, "admin extensions registered");
        }

        router
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rinfra_core::config::RinfraConfig;

    #[test]
    fn test_admin_builder_default() {
        let state = Arc::new(AppState::new(RinfraConfig::default()));
        let _router = AdminBuilder::new("nonexistent").build(state);
    }

    #[test]
    fn test_admin_builder_with_menu() {
        let state = Arc::new(AppState::new(RinfraConfig::default()));
        let _router = AdminBuilder::new("nonexistent")
            .menu(MenuEntry {
                path: "/test".into(),
                name: "Test".into(),
                icon: "T".into(),
                category: "test".into(),
                data_url: "/api/admin/test".into(),
            })
            .build(state);
    }

    #[test]
    fn test_admin_router_backward_compat() {
        let state = Arc::new(AppState::new(RinfraConfig::default()));
        let _router = admin_router("nonexistent", state);
    }
}

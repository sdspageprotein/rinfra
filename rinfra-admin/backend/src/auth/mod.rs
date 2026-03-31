mod middleware;
mod store;

use std::sync::Arc;
use rinfra_core::config::AdminAuthConfig;

pub use middleware::AdminAuthLayer;
pub use store::{AdminRole, AdminTokenInfo, AdminTokenRecord, AdminTokenStore};

/// Bootstrap admin authentication: load token store, handle env-based root
/// token, and auto-generate root token on first run.
/// Returns the initialized store ready for use with `AdminBuilder::with_auth()`.
pub fn bootstrap_admin_auth(
    config: &AdminAuthConfig,
) -> Result<Arc<AdminTokenStore>, String> {
    let store = Arc::new(AdminTokenStore::load(&config.token_file)?);

    if let Ok(env_token) = std::env::var(&config.root_token_env) {
        if !env_token.is_empty() && !store.has_root() {
            store.create_token_with_value(
                AdminRole::Root,
                "root (from env)".into(),
                &env_token,
                None,
            )?;
            tracing::info!(
                env = %config.root_token_env,
                "root token loaded from environment variable"
            );
            return Ok(store);
        }
    }

    if !store.has_root() {
        let (token, _info) = store.create_token(AdminRole::Root, "root".into(), None)?;

        eprintln!();
        eprintln!("╔══════════════════════════════════════════════════════════════════════╗");
        eprintln!("║  ADMIN ROOT TOKEN (save this!):                                     ║");
        eprintln!("║  {:<64}  ║", token);
        eprintln!("║  This is shown ONCE at first startup.                               ║");
        eprintln!("╚══════════════════════════════════════════════════════════════════════╝");
        eprintln!();

        tracing::warn!("admin root token generated — see stderr output above");
    }

    Ok(store)
}

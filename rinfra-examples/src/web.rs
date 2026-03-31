use std::sync::Arc;

use axum::extract::State;
use axum::response::Html;
use axum::routing::get;
use axum::Router;
use rinfra_core::AppState;

pub fn web_router(state: Arc<AppState>) -> Router {
    Router::new()
        .route("/", get(index_handler))
        .with_state(state)
}

async fn index_handler(State(state): State<Arc<AppState>>) -> Html<String> {
    let app_name = &state.config.app.name;
    let app_version = &state.config.app.version;
    let node_id = &state.config.plugins.cluster.node_id;
    let uptime = state.started_at.elapsed();

    let html = format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{app_name}</title>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: linear-gradient(135deg, #667eea 0%, #764ba2 100%);
            min-height: 100vh;
            display: flex;
            align-items: center;
            justify-content: center;
            color: #333;
        }}
        .card {{
            background: white;
            border-radius: 16px;
            padding: 48px;
            box-shadow: 0 20px 60px rgba(0,0,0,0.2);
            max-width: 480px;
            width: 90%;
            text-align: center;
        }}
        h1 {{ font-size: 2rem; margin-bottom: 8px; color: #1a1a2e; }}
        .subtitle {{ color: #888; margin-bottom: 32px; font-size: 0.95rem; }}
        .info {{ text-align: left; }}
        .info-row {{
            display: flex;
            justify-content: space-between;
            padding: 12px 0;
            border-bottom: 1px solid #eee;
            font-size: 0.9rem;
        }}
        .info-row:last-child {{ border-bottom: none; }}
        .label {{ color: #888; }}
        .value {{ font-weight: 600; color: #1a1a2e; }}
        .badge {{
            display: inline-block;
            background: #e8f5e9;
            color: #2e7d32;
            padding: 2px 10px;
            border-radius: 12px;
            font-size: 0.8rem;
        }}
    </style>
</head>
<body>
    <div class="card">
        <h1>rinfra</h1>
        <p class="subtitle">Modular Rust Infrastructure Framework</p>
        <div class="info">
            <div class="info-row">
                <span class="label">Service</span>
                <span class="value">{app_name}</span>
            </div>
            <div class="info-row">
                <span class="label">Version</span>
                <span class="value">{app_version}</span>
            </div>
            <div class="info-row">
                <span class="label">Node ID</span>
                <span class="value">{node_id}</span>
            </div>
            <div class="info-row">
                <span class="label">Uptime</span>
                <span class="value">{uptime:.0?}</span>
            </div>
            <div class="info-row">
                <span class="label">Status</span>
                <span class="value"><span class="badge">Online</span></span>
            </div>
        </div>
    </div>
</body>
</html>"#,
    );

    Html(html)
}

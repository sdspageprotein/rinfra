use rinfra_plugins::RunOptions;

#[tokio::main]
async fn main() {
    rinfra_plugins::run(
        RunOptions::new().http_router("main", |state| {
            let admin_config = state.config.plugins.admin.clone();
            rinfra_admin::AdminBuilder::new(&admin_config.static_dir)
                .with_auth_config(&admin_config.auth)
                .build(state)
        }),
    )
    .await;
}

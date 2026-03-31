use rinfra_plugins::RunOptions;

#[tokio::main]
async fn main() {
    rinfra_plugins::run(
        RunOptions::new()
            .http_router("main", |state| rinfra_examples::web::web_router(state))
            .metadata(vec![("service_type", "web")]),
    )
    .await;
}

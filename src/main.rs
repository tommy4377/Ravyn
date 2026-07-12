use clap::Parser;
use ravyn::{Ravyn, api, config::Config};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "ravyn=info,tower_http=info".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::parse();
    let app = Ravyn::bootstrap(config).await?;
    app.manager.clone().start_workers().await?;
    api::serve(app).await?;
    Ok(())
}

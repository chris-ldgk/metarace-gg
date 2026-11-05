use anyhow::{Context, Result};
use metarace_rre_manager::{api, config::Config, state::AppState};
use tracing_subscriber::{EnvFilter, layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<()> {
    let config = Config::load().context("load configuration")?;
    init_logging(&config);
    tracing::info!(
        version = env!("CARGO_PKG_VERSION"),
        bind = %config.bind,
        data_dir = %config.data_dir.display(),
        game_port = config.game_port,
        launcher = %config.game_launcher,
        "starting metarace rre manager"
    );
    if config.allow_insecure && config.control_token.is_none() {
        tracing::warn!("control endpoints are unauthenticated (METARACE_ALLOW_INSECURE)");
    }
    if config.result.is_none() {
        tracing::warn!("METARACE_RESULT_URL is not set; results are stored locally only");
    }
    if config.dedi_ip.is_none() {
        tracing::warn!(
            "could not determine an IPv4 bind address; set METARACE_DEDI_IP or the game server will bind IPv6-only"
        );
    }

    let bind = config.bind;
    let state = AppState::build(config).context("build application state")?;
    let app = api::router(state.clone());

    let listener = tokio::net::TcpListener::bind(bind)
        .await
        .with_context(|| format!("bind {bind}"))?;
    tracing::info!(address = %listener.local_addr()?, "control API listening");

    state.start().await.context("start the dedicated server")?;

    let result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;
    tracing::info!("shutting down");
    state.shutdown().await;
    result.context("serve HTTP")?;
    Ok(())
}

fn init_logging(config: &Config) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(config.log_level.clone()));
    tracing_subscriber::registry()
        .with(filter)
        .with(
            tracing_subscriber::fmt::layer()
                .with_writer(std::io::stderr)
                .with_target(true),
        )
        .init();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}

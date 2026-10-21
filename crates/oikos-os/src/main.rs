#![forbid(unsafe_code)]

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use oikos_os::cli::Cli;
use oikos_os::config::Config;
use oikos_os::history::History;
use oikos_os::hub::Hub;
use oikos_os::{http, logging, mcp, mqtt};
use tokio::sync::mpsc;

/// Publishes waiting for the MQTT task before the hub refuses new ones.
const PUBLISH_QUEUE: usize = 64;
/// How long the Disconnect packet gets to reach the wire before the task is dropped.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse_args();
    match cli {
        Cli::Run { config } => {
            let cfg = Config::load(config.as_deref())?;
            cfg.flows
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            logging::init(&cfg.log);
            tracing::info!(host = %cfg.mqtt.host, port = cfg.mqtt.port, "starting oikos-os");

            let (publish_tx, publish_rx) = mpsc::channel(PUBLISH_QUEUE);
            let hub = Hub::new(&cfg, publish_tx);
            if cfg.history.enabled() {
                match History::start(&cfg.history, hub.zone()) {
                    Ok(history) => hub.set_history(history),
                    Err(e) => tracing::warn!(
                        error = %e,
                        "history disabled: lazy pool could not be built; the house keeps running"
                    ),
                }
            }
            let engine = hub.start_engine();
            let (link, mut link_task) = mqtt::start(
                cfg.mqtt.clone(),
                cfg.topics.clone(),
                cfg.push.clone(),
                Arc::clone(&hub),
                publish_rx,
            );
            tokio::spawn(oikos_os::groups::migrate::spawn(Arc::clone(&hub)));
            let http_task = http::serve(&cfg.http, http::router(Arc::clone(&hub))).await?;

            tokio::signal::ctrl_c().await?;
            tracing::info!("shutting down");
            if link.disconnect() {
                if tokio::time::timeout(SHUTDOWN_GRACE, &mut link_task)
                    .await
                    .is_err()
                {
                    tracing::warn!("the broker did not take the disconnect in time");
                    link_task.abort();
                }
            } else {
                link_task.abort();
            }
            engine.abort();
            http_task.abort();
            tracing::info!("stopped");
            Ok(ExitCode::SUCCESS)
        }
        Cli::McpStdio { server } => {
            let (cfg, load_err) = match Config::load(None) {
                Ok(cfg) => (cfg, None),
                Err(e) => (Config::default(), Some(e)),
            };
            logging::init(&cfg.log);
            if let Some(e) = load_err {
                tracing::warn!(error = %e, "ignoring invalid config, using defaults for logging");
            }
            mcp::run_stdio_proxy(server).await?;
            Ok(ExitCode::SUCCESS)
        }
    }
}

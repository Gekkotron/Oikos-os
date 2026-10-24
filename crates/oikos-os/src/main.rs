#![forbid(unsafe_code)]

use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use oikos_os::cli::Cli;
use oikos_os::config::{AclSource, Config};
use oikos_os::history::History;
use oikos_os::hub::Hub;
use oikos_os::{http, logging, mcp, mqtt};
use tokio::sync::mpsc;

/// Publishes waiting for the MQTT task before the hub refuses new ones.
const PUBLISH_QUEUE: usize = 64;
/// Log lines waiting for the ACL watchdog.
const ACL_LINES: usize = 256;
/// How long the Disconnect packet gets to reach the wire before the task is dropped.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// Feeds the ACL watchdog from the Mosquitto log file when that is its source: the tail task and
/// the task that hands its lines to the hub.
fn spawn_acl_tail(cfg: &Config, hub: &Arc<Hub>) -> Vec<tokio::task::JoinHandle<()>> {
    let acl = &cfg.watchdog.acl;
    let (true, AclSource::File, Some(path)) = (acl.enabled, acl.source, acl.log_file.clone())
    else {
        return Vec::new();
    };
    tracing::info!(path = %path.display(), "watching the broker log for ACL denials");
    let (tx, mut rx) = mpsc::channel::<String>(ACL_LINES);
    let tail = oikos_watchdog::tail(path, tx);
    let hub = Arc::clone(hub);
    let feed = tokio::spawn(async move {
        while let Some(line) = rx.recv().await {
            hub.on_acl_line(&line);
        }
    });
    vec![tail, feed]
}

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse_args();
    match cli {
        Cli::Run { config } => {
            let cfg = Config::load(config.as_deref())?;
            cfg.flows
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            cfg.watchdog
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            cfg.climate
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            cfg.alarm
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            cfg.weather
                .validate()
                .map_err(|e| anyhow::anyhow!("invalid config: {e}"))?;
            cfg.solcast
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
            let climate = hub.start_climate();
            let push_retry = hub.push().spawn_retry_worker();
            let acl_tail = spawn_acl_tail(&cfg, &hub);
            let (link, mut link_task) = mqtt::start(
                cfg.mqtt.clone(),
                cfg.topics.clone(),
                Arc::clone(&hub),
                publish_rx,
            );
            let weather = hub.start_weather();
            let solar = hub.start_solar();
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
            if let Some(task) = &climate {
                task.abort();
            }
            if let Some(task) = &weather {
                task.abort();
            }
            if let Some(task) = &solar {
                task.abort();
            }
            push_retry.abort();
            for task in &acl_tail {
                task.abort();
            }
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

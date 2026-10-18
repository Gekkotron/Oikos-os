#![forbid(unsafe_code)]

use std::process::ExitCode;

use oikos_os::cli::Cli;
use oikos_os::config::Config;
use oikos_os::logging;

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    let cli = Cli::parse_args();
    match cli {
        Cli::Run { config } => {
            let cfg = Config::load(config.as_deref())?;
            logging::init(&cfg.log);
            tracing::info!(host = %cfg.mqtt.host, port = cfg.mqtt.port, "starting oikos-os");
            Ok(ExitCode::SUCCESS)
        }
        Cli::McpStdio { server } => {
            let cfg = Config::load(None)?;
            logging::init(&cfg.log);
            tracing::error!(%server, "mcp-stdio is not implemented yet");
            Ok(ExitCode::from(2))
        }
    }
}

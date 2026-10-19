use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(name = "oikos-os", version, about = "Oikos home-automation hub")]
struct RawCli {
    #[command(subcommand)]
    command: Option<RawCommand>,
    /// Config file used when no subcommand is given (same as `run --config`).
    #[arg(long, global = true, value_name = "PATH")]
    config: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
enum RawCommand {
    /// Run the hub (default).
    Run,
    /// Serve MCP over stdio, proxying to a running hub.
    McpStdio {
        /// Base URL of the hub, e.g. http://localhost:8081
        #[arg(long)]
        server: String,
    },
}

/// Resolved command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cli {
    Run { config: Option<PathBuf> },
    McpStdio { server: String },
}

impl Cli {
    pub fn parse_args() -> Cli {
        Self::from_raw(RawCli::parse())
    }

    pub fn try_parse_from<I, T>(args: I) -> Result<Cli, clap::Error>
    where
        I: IntoIterator<Item = T>,
        T: Into<std::ffi::OsString> + Clone,
    {
        RawCli::try_parse_from(args).map(Self::from_raw)
    }

    fn from_raw(raw: RawCli) -> Cli {
        match raw.command {
            None | Some(RawCommand::Run) => Cli::Run { config: raw.config },
            Some(RawCommand::McpStdio { server }) => Cli::McpStdio { server },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_is_default() {
        assert_eq!(
            Cli::try_parse_from(["oikos-os"]).unwrap(),
            Cli::Run { config: None }
        );
        assert_eq!(
            Cli::try_parse_from(["oikos-os", "run", "--config", "x.toml"]).unwrap(),
            Cli::Run {
                config: Some("x.toml".into())
            }
        );
        assert_eq!(
            Cli::try_parse_from(["oikos-os", "--config", "y.toml"]).unwrap(),
            Cli::Run {
                config: Some("y.toml".into())
            }
        );
    }

    #[test]
    fn mcp_stdio_takes_server() {
        assert_eq!(
            Cli::try_parse_from(["oikos-os", "mcp-stdio", "--server", "http://h:1"]).unwrap(),
            Cli::McpStdio {
                server: "http://h:1".into()
            }
        );
    }
}

use tracing_subscriber::EnvFilter;

use crate::config::LogConfig;

/// Installs the global subscriber. `RUST_LOG` wins over `cfg.level`; an
/// invalid level falls back to `info`. Output goes to stderr.
pub fn init(cfg: &LogConfig) {
    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| EnvFilter::try_new(&cfg.level))
        .unwrap_or_else(|_| EnvFilter::new("info"));
    let builder = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr);
    if cfg.json {
        let _ = builder.json().try_init();
    } else {
        let _ = builder.pretty().try_init();
    }
}

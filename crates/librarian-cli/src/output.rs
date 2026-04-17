//! Output formatting: tracing initialisation and display modes.

use tracing_subscriber::{fmt, EnvFilter};

pub fn init_tracing(verbose: bool, json: bool, quiet: bool) -> anyhow::Result<()> {
    let filter = if verbose {
        EnvFilter::new("debug")
    } else if quiet {
        EnvFilter::new("error")
    } else {
        EnvFilter::new("info")
    };

    if json {
        fmt()
            .json()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    } else {
        fmt()
            .with_env_filter(filter)
            .with_target(false)
            .init();
    }

    Ok(())
}

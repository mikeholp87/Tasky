use tracing_subscriber::EnvFilter;

/// Initialises structured logging to stderr. Debug builds log at `debug` for
/// this crate; release builds default to `warn`. `RUST_LOG` overrides both.
pub fn init() {
    let default = if cfg!(debug_assertions) {
        "warn,taskmanager_lib=debug"
    } else {
        "warn"
    };
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default));
    // Ignore the error if a subscriber is already installed (e.g. in tests).
    let _ = tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .try_init();
}

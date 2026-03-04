use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

pub(crate) fn init_tracing() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into());
    let ansi = std::io::stderr().is_terminal();
    let _ = tracing_subscriber::fmt()
        .with_env_filter(env_filter)
        .with_ansi(ansi)
        .with_target(true)
        .with_level(true)
        .without_time()
        .compact()
        .try_init();
}

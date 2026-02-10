mod app;
mod cli;
mod config;
mod dir_mode;
mod errors;
mod fetch_runtime;
mod file_mode;
mod image_store;
mod logging;
mod mode_params;
mod nfo;
mod script_info;
mod source_priority;
mod source_runner;
mod video_parts;
mod watch_mode;

fn main() {
    logging::init_tracing();
    if let Err(error) = app::run() {
        error.report();
        std::process::exit(error.exit_code());
    }
}

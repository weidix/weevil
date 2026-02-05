mod app;
mod cli;
mod errors;
mod file_mode;
mod logging;
mod nfo;

fn main() {
    logging::init_tracing();
    if let Err(error) = app::run() {
        error.report();
        std::process::exit(error.exit_code());
    }
}

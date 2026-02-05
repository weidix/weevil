use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
#[command(
    name = "weevil",
    about = "Command-line toolkit for scraping NFO metadata",
    arg_required_else_help = true
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    #[command(
        name = "name",
        about = "Generate NFO output by name with a Lua script.",
        after_help = "Notes:\n  The Lua run function must return either:\n    - a table matching the NFO movie schema, or\n    - a string containing raw NFO XML."
    )]
    Name {
        #[arg(long, short = 'n', value_name = "NAME")]
        name: String,
        #[arg(long, short = 's', value_name = "SCRIPT")]
        script: PathBuf,
        #[arg(long, short = 'o', value_name = "OUTPUT")]
        output: PathBuf,
    },
    #[command(
        name = "file",
        about = "Generate NFO output from a file, then rename and move assets."
    )]
    File {
        #[arg(long, short = 'i', value_name = "FILE")]
        input: PathBuf,
        #[arg(long, short = 's', value_name = "SCRIPT")]
        script: PathBuf,
        #[arg(long, short = 'o', value_name = "OUTPUT_DIR")]
        output_dir: PathBuf,
        #[arg(
            long,
            value_name = "REMOVE",
            value_delimiter = ',',
            help = "Comma-separated tokens to remove from the input filename before passing to Lua."
        )]
        input_name_remove: Vec<String>,
        #[arg(long, value_name = "FORMAT", default_value = "")]
        file_format: String,
        #[arg(long, value_name = "FORMAT", default_value = "")]
        folder_format: String,
    },
    #[command(name = "dir", about = "Reserved: directory to directory mode.")]
    Dir,
    #[command(name = "watch", about = "Reserved: directory watch mode.")]
    Watch,
}

#[cfg(test)]
#[path = "tests/cli.rs"]
mod tests;

use std::path::PathBuf;

use super::*;

#[test]
fn parse_name_command() {
    let cli = Cli::try_parse_from([
        "weevil",
        "name",
        "--name",
        "Spirited Away",
        "--script",
        "script.lua",
        "--output",
        "movie.nfo",
    ])
    .expect("expected command");

    if let Command::Name {
        name,
        script,
        output,
    } = cli.command
    {
        assert_eq!(name, "Spirited Away");
        assert_eq!(script, PathBuf::from("script.lua"));
        assert_eq!(output, PathBuf::from("movie.nfo"));
    } else {
        panic!("expected name command");
    }
}

#[test]
fn parse_missing_mode_is_help() {
    let error = Cli::try_parse_from(["weevil"]).expect_err("expected error");
    assert!(matches!(
        error.kind(),
        clap::error::ErrorKind::DisplayHelp
            | clap::error::ErrorKind::DisplayHelpOnMissingArgumentOrSubcommand
    ));
}

#[test]
fn parse_unknown_mode() {
    let error = Cli::try_parse_from(["weevil", "oops"]).expect_err("expected error");
    assert!(matches!(
        error.kind(),
        clap::error::ErrorKind::UnknownArgument | clap::error::ErrorKind::InvalidSubcommand
    ));
}

#[test]
fn parse_file_mode() {
    let cli = Cli::try_parse_from([
        "weevil",
        "file",
        "--input",
        "movie.mkv",
        "--script",
        "script.lua",
        "--output-dir",
        "output",
    ])
    .expect("expected command");
    assert!(matches!(cli.command, Command::File { .. }));
}

#[test]
fn parse_file_mode_input_name_remove() {
    let cli = Cli::try_parse_from([
        "weevil",
        "file",
        "--input",
        "movie.mkv",
        "--script",
        "script.lua",
        "--output-dir",
        "output",
        "--input-name-remove",
        "1080p,WEB-DL",
    ])
    .expect("expected command");

    if let Command::File {
        input_name_remove, ..
    } = cli.command
    {
        assert_eq!(input_name_remove, vec!["1080p", "WEB-DL"]);
    } else {
        panic!("expected file command");
    }
}

#[test]
fn parse_extra_args() {
    let error = Cli::try_parse_from([
        "weevil",
        "name",
        "--name",
        "Name",
        "--script",
        "script.lua",
        "--output",
        "movie.nfo",
        "extra",
    ])
    .expect_err("expected error");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
}

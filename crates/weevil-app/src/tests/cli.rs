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
        assert_eq!(script, Some(PathBuf::from("script.lua")));
        assert_eq!(output, Some(PathBuf::from("movie.nfo")));
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
        "--output",
        "library/{title}",
        "--input-name-rule",
        "1080p,WEB-DL",
        "--folder-multi",
        "hard-link",
    ])
    .expect("expected command");

    if let Command::File {
        input,
        script,
        output,
        input_name_rules,
        folder_multi,
    } = cli.command
    {
        assert_eq!(input, PathBuf::from("movie.mkv"));
        assert_eq!(script, Some(PathBuf::from("script.lua")));
        assert_eq!(output, Some("library/{title}".to_string()));
        assert_eq!(input_name_rules, vec!["1080p,WEB-DL"]);
        assert_eq!(folder_multi, Some(FolderMultiStrategy::HardLink));
    } else {
        panic!("expected file command");
    }
}

#[test]
fn parse_dir_mode() {
    let cli = Cli::try_parse_from([
        "weevil",
        "dir",
        "--input",
        "videos",
        "--script",
        "script.lua",
        "--output",
        "library/{title}",
        "--input-name-rule",
        "regex:\\[[^\\]]+\\]",
        "--folder-multi",
        "soft-link",
        "--max-depth",
        "2",
    ])
    .expect("expected command");

    if let Command::Dir {
        input,
        script,
        output,
        input_name_rules,
        folder_multi,
        max_depth,
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
    } = cli.command
    {
        assert_eq!(input, Some(PathBuf::from("videos")));
        assert_eq!(script, Some(PathBuf::from("script.lua")));
        assert_eq!(output, Some("library/{title}".to_string()));
        assert_eq!(input_name_rules, vec!["regex:\\[[^\\]]+\\]"]);
        assert_eq!(folder_multi, Some(FolderMultiStrategy::SoftLink));
        assert_eq!(max_depth, Some(2));
        assert_eq!(fetch_threads, None);
        assert_eq!(throttle_same_script, None);
        assert_eq!(script_throttle_base_ms, None);
    } else {
        panic!("expected dir command");
    }
}

#[test]
fn parse_watch_mode() {
    let cli = Cli::try_parse_from([
        "weevil",
        "watch",
        "--input",
        "incoming",
        "--max-depth",
        "-1",
    ])
    .expect("expected command");

    if let Command::Watch {
        input,
        max_depth,
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
        ..
    } = cli.command
    {
        assert_eq!(input, Some(PathBuf::from("incoming")));
        assert_eq!(max_depth, Some(-1));
        assert_eq!(fetch_threads, None);
        assert_eq!(throttle_same_script, None);
        assert_eq!(script_throttle_base_ms, None);
    } else {
        panic!("expected watch command");
    }
}

#[test]
fn parse_dir_multithread_options() {
    let cli = Cli::try_parse_from([
        "weevil",
        "dir",
        "--input",
        "videos",
        "--fetch-threads",
        "0",
        "--throttle-same-script",
        "true",
        "--script-throttle-base-ms",
        "1600",
    ])
    .expect("expected command");

    if let Command::Dir {
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
        ..
    } = cli.command
    {
        assert_eq!(fetch_threads, Some(0));
        assert_eq!(throttle_same_script, Some(true));
        assert_eq!(script_throttle_base_ms, Some(1600));
    } else {
        panic!("expected dir command");
    }
}

#[test]
fn parse_config_option_short() {
    let cli =
        Cli::try_parse_from(["weevil", "-c", "custom.toml", "dir"]).expect("expected command");
    assert_eq!(cli.config, Some(PathBuf::from("custom.toml")));
    assert!(matches!(cli.command, Command::Dir { .. }));
}

#[test]
fn parse_config_option_long() {
    let cli = Cli::try_parse_from(["weevil", "--config", "custom.toml", "watch"])
        .expect("expected command");
    assert_eq!(cli.config, Some(PathBuf::from("custom.toml")));
    assert!(matches!(cli.command, Command::Watch { .. }));
}

#[test]
fn parse_extra_args() {
    let error = Cli::try_parse_from(["weevil", "name", "--name", "Name", "extra"])
        .expect_err("expected error");
    assert_eq!(error.kind(), clap::error::ErrorKind::UnknownArgument);
}

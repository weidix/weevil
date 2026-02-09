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
        scripts,
        output,
        multi_source,
        save_images,
        multi_source_max_sources,
    } = cli.command
    {
        assert_eq!(name, "Spirited Away");
        assert_eq!(scripts, vec![PathBuf::from("script.lua")]);
        assert_eq!(output, Some(PathBuf::from("movie.nfo")));
        assert!(!multi_source);
        assert!(!save_images);
        assert_eq!(multi_source_max_sources, None);
    } else {
        panic!("expected name command");
    }
}

#[test]
fn parse_name_multi_scripts_and_multi_source() {
    let cli = Cli::try_parse_from([
        "weevil",
        "name",
        "--name",
        "Movie",
        "--script",
        "a.lua",
        "--script",
        "b.lua",
        "--multi-source",
        "--multi-source-max-sources",
        "3",
    ])
    .expect("expected command");

    if let Command::Name {
        scripts,
        multi_source,
        save_images,
        multi_source_max_sources,
        ..
    } = cli.command
    {
        assert_eq!(
            scripts,
            vec![PathBuf::from("a.lua"), PathBuf::from("b.lua")]
        );
        assert!(multi_source);
        assert!(!save_images);
        assert_eq!(multi_source_max_sources, Some(3));
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
        scripts,
        output,
        input_name_rules,
        folder_multi,
        multi_source,
        save_images,
        multi_source_max_sources,
    } = cli.command
    {
        assert_eq!(input, PathBuf::from("movie.mkv"));
        assert_eq!(scripts, vec![PathBuf::from("script.lua")]);
        assert_eq!(output, Some("library/{title}".to_string()));
        assert_eq!(input_name_rules, vec!["1080p,WEB-DL"]);
        assert_eq!(folder_multi, Some(FolderMultiStrategy::HardLink));
        assert!(!multi_source);
        assert!(!save_images);
        assert_eq!(multi_source_max_sources, None);
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
        scripts,
        output,
        input_name_rules,
        folder_multi,
        max_depth,
        fetch_threads,
        throttle_same_script,
        script_throttle_base_ms,
        multi_source,
        save_images,
        multi_source_max_sources,
    } = cli.command
    {
        assert_eq!(input, Some(PathBuf::from("videos")));
        assert_eq!(scripts, vec![PathBuf::from("script.lua")]);
        assert_eq!(output, Some("library/{title}".to_string()));
        assert_eq!(input_name_rules, vec!["regex:\\[[^\\]]+\\]"]);
        assert_eq!(folder_multi, Some(FolderMultiStrategy::SoftLink));
        assert_eq!(max_depth, Some(2));
        assert_eq!(fetch_threads, None);
        assert!(!throttle_same_script);
        assert_eq!(script_throttle_base_ms, None);
        assert!(!multi_source);
        assert!(!save_images);
        assert_eq!(multi_source_max_sources, None);
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
        multi_source,
        save_images,
        multi_source_max_sources,
        ..
    } = cli.command
    {
        assert_eq!(input, Some(PathBuf::from("incoming")));
        assert_eq!(max_depth, Some(-1));
        assert_eq!(fetch_threads, None);
        assert!(!throttle_same_script);
        assert_eq!(script_throttle_base_ms, None);
        assert!(!multi_source);
        assert!(!save_images);
        assert_eq!(multi_source_max_sources, None);
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
        assert!(throttle_same_script);
        assert_eq!(script_throttle_base_ms, Some(1600));
    } else {
        panic!("expected dir command");
    }
}

#[test]
fn parse_file_save_images_option() {
    let cli = Cli::try_parse_from(["weevil", "file", "--input", "movie.mkv", "--save-images"])
        .expect("expected command");

    if let Command::File { save_images, .. } = cli.command {
        assert!(save_images);
    } else {
        panic!("expected file command");
    }
}

#[test]
fn parse_name_save_images_option() {
    let cli = Cli::try_parse_from(["weevil", "name", "--name", "movie", "--save-images"])
        .expect("expected command");

    if let Command::Name { save_images, .. } = cli.command {
        assert!(save_images);
    } else {
        panic!("expected name command");
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

#[test]
fn parse_scripts_command() {
    let cli = Cli::try_parse_from([
        "weevil", "scripts", "--script", "a.lua", "--script", "b.lua",
    ])
    .expect("expected command");

    if let Command::Scripts { scripts } = cli.command {
        assert_eq!(
            scripts,
            vec![PathBuf::from("a.lua"), PathBuf::from("b.lua")]
        );
    } else {
        panic!("expected scripts command");
    }
}

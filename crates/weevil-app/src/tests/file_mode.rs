use std::collections::HashSet;

use super::*;

#[test]
fn subtitle_suffix_matches_language_suffix() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(subtitle_suffix("Movie", "Movie"), Some(String::new()));
    assert_eq!(subtitle_suffix("Movie", "Other"), None);
}

#[test]
fn subtitle_suffix_matches_normalized_names() {
    assert_eq!(
        subtitle_suffix("My-Movie", "my movie.ZH"),
        Some(".zh-CN".to_string())
    );
}

#[test]
fn subtitle_suffix_allows_short_name_matching() {
    assert_eq!(
        subtitle_suffix("Movie.2020.1080p", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie.Special.Edition", "Movie"),
        Some(String::new())
    );
}

#[test]
fn subtitle_suffix_ignores_noise_tokens() {
    assert_eq!(
        subtitle_suffix("Movie.2020.1080p.BluRay.x264", "Movie.zh"),
        Some(".zh-CN".to_string())
    );
}

#[test]
fn subtitle_suffix_rejects_too_short_short_name() {
    assert_eq!(subtitle_suffix("Up.2009", "U"), None);
}

#[test]
fn subtitle_suffix_rejects_unrelated_names() {
    assert_eq!(subtitle_suffix("Movie.One", "Movie.Two.zh"), None);
    assert_eq!(subtitle_suffix("The.Room", "The.Roommate"), None);
}

#[test]
fn subtitle_suffix_normalizes_language_aliases() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh_CN"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.ch"),
        Some(".zh-CN".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh-TW"),
        Some(".zh-TW".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.en_US"),
        Some(".en-US".to_string())
    );
    assert_eq!(
        subtitle_suffix("Movie", "Movie.pt_br"),
        Some(".pt-BR".to_string())
    );
}

#[test]
fn subtitle_suffix_keeps_language_and_other_suffix_parts() {
    assert_eq!(
        subtitle_suffix("Movie", "Movie.zh_CN.forced"),
        Some(".zh-CN.forced".to_string())
    );
}

#[test]
fn select_output_paths_first_strategy_keeps_only_primary() {
    let outputs = vec![PathBuf::from("one/Title"), PathBuf::from("two/Title")];
    let selected =
        select_output_paths(outputs, "{title}", MultiFolderStrategy::First).expect("selected");
    assert_eq!(selected.primary.dir, PathBuf::from("one"));
    assert_eq!(selected.primary.file_base, "Title");
    assert!(selected.extras.is_empty());
}

#[test]
fn select_output_paths_non_first_keeps_extras() {
    let outputs = vec![PathBuf::from("one/Title"), PathBuf::from("two/Title")];
    let selected =
        select_output_paths(outputs, "{title}", MultiFolderStrategy::HardLink).expect("selected");
    assert_eq!(selected.primary.dir, PathBuf::from("one"));
    assert_eq!(selected.primary.file_base, "Title");
    assert_eq!(selected.extras.len(), 1);
    assert_eq!(selected.extras[0].dir, PathBuf::from("two"));
    assert_eq!(selected.extras[0].file_base, "Title");
}

#[test]
fn build_output_targets_generates_split_part_video_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_dir = dir.path().join("input");
    std::fs::create_dir_all(&input_dir).expect("create input dir");
    std::fs::write(input_dir.join("Movie-CD1.mkv"), b"part1").expect("write part1");
    std::fs::write(input_dir.join("Movie-CD2.mp4"), b"part2").expect("write part2");

    let output = OutputPath {
        dir: PathBuf::from("library"),
        file_base: "Movie".to_string(),
    };
    let parts = vec![
        crate::video_parts::VideoInputPart {
            path: input_dir.join("Movie-CD1.mkv"),
            input_stem: "Movie-CD1".to_string(),
            output_suffix: ".part01".to_string(),
        },
        crate::video_parts::VideoInputPart {
            path: input_dir.join("Movie-CD2.mp4"),
            input_stem: "Movie-CD2".to_string(),
            output_suffix: ".part02".to_string(),
        },
    ];

    let subtitle_plans = vec![SubtitlePlan {
        path: input_dir.join("Movie.zh.srt"),
        target_base: ".zh-CN".to_string(),
        sort_key: "1:movie.zh.srt".to_string(),
    }];
    std::fs::write(&subtitle_plans[0].path, b"sub").expect("write subtitle");

    let targets = build_output_targets(&output, &parts, &subtitle_plans, &[]).expect("targets");
    assert_eq!(
        targets.videos,
        vec![
            PathBuf::from("library/Movie.part01.mkv"),
            PathBuf::from("library/Movie.part02.mp4"),
        ]
    );
    assert_eq!(targets.subtitles.len(), 1);
    assert_eq!(
        targets.subtitles[0].path,
        PathBuf::from("library/Movie.zh-CN.srt")
    );
}

#[test]
fn collect_group_subtitle_plans_supports_group_level_subtitle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_dir = dir.path().join("input");
    std::fs::create_dir_all(&input_dir).expect("create input dir");

    let cd1 = input_dir.join("Movie-CD1.mkv");
    let cd2 = input_dir.join("Movie-CD2.mkv");
    std::fs::write(&cd1, b"part1").expect("write cd1");
    std::fs::write(&cd2, b"part2").expect("write cd2");
    std::fs::write(input_dir.join("Movie-CD1.zh.srt"), b"sub1").expect("write sub1");
    std::fs::write(input_dir.join("Movie.zh.srt"), b"sub-group").expect("write group sub");

    let group = crate::video_parts::group_video_inputs(&[cd1, cd2])
        .expect("group")
        .into_iter()
        .next()
        .expect("one group");

    let plans = collect_group_subtitle_plans(&group).expect("plans");
    assert_eq!(plans.len(), 2);
    assert!(plans.iter().any(|plan| plan.target_base == ".part01.zh-CN"));
    assert!(plans.iter().any(|plan| plan.target_base == ".zh-CN"));
}

#[test]
fn collect_group_subtitle_plans_keeps_split_and_group_subtitle() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_dir = dir.path().join("input");
    std::fs::create_dir_all(&input_dir).expect("create input dir");

    let cd1 = input_dir.join("Movie-CD1.mkv");
    let cd2 = input_dir.join("Movie-CD2.mkv");
    std::fs::write(&cd1, b"part1").expect("write cd1");
    std::fs::write(&cd2, b"part2").expect("write cd2");
    std::fs::write(input_dir.join("Movie-CD1.zh.srt"), b"sub1").expect("write sub1");
    std::fs::write(input_dir.join("Movie-CD2.zh.srt"), b"sub2").expect("write sub2");
    std::fs::write(input_dir.join("Movie.zh.srt"), b"sub-group").expect("write group sub");

    let group = crate::video_parts::group_video_inputs(&[cd1, cd2])
        .expect("group")
        .into_iter()
        .next()
        .expect("one group");

    let plans = collect_group_subtitle_plans(&group).expect("plans");
    assert_eq!(plans.len(), 3);
    let part_zh_count = plans
        .iter()
        .filter(|plan| plan.target_base.ends_with(".zh-CN") && plan.target_base.contains(".part"))
        .count();
    assert_eq!(part_zh_count, 2);
    assert!(plans.iter().any(|plan| plan.target_base == ".zh-CN"));
}

#[test]
fn collect_group_subtitle_plans_group_only_subtitle_not_duplicated_per_part() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_dir = dir.path().join("input");
    std::fs::create_dir_all(&input_dir).expect("create input dir");

    let cd1 = input_dir.join("Movie-CD1.mkv");
    let cd2 = input_dir.join("Movie-CD2.mkv");
    std::fs::write(&cd1, b"part1").expect("write cd1");
    std::fs::write(&cd2, b"part2").expect("write cd2");
    let group_sub = input_dir.join("Movie.zh.srt");
    std::fs::write(&group_sub, b"sub-group").expect("write group sub");

    let group = crate::video_parts::group_video_inputs(&[cd1, cd2])
        .expect("group")
        .into_iter()
        .next()
        .expect("one group");

    let plans = collect_group_subtitle_plans(&group).expect("plans");
    assert_eq!(plans.len(), 1);
    assert_eq!(plans[0].target_base, ".zh-CN");

    let unique_paths = plans
        .iter()
        .map(|plan| plan.path.clone())
        .collect::<HashSet<_>>();
    assert_eq!(unique_paths.len(), 1);
}

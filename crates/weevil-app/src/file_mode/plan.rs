use std::path::{Path, PathBuf};

use tokio::fs;

use crate::errors::AppError;
use crate::video_parts::{self, VideoInputGroup, VideoInputPart};

use super::MultiFolderStrategy;
use super::fs_ops::create_link;
use super::naming::build_file_name;
use super::subtitle_match::subtitle_suffix;

const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];

pub(super) struct OutputTargets {
    pub(super) videos: Vec<PathBuf>,
    pub(super) nfo: PathBuf,
    pub(super) subtitles: Vec<SubtitleTarget>,
    pub(super) images: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
pub(super) struct SubtitlePlan {
    pub(super) path: PathBuf,
    pub(super) target_base: String,
    pub(super) sort_key: String,
}

#[derive(Debug, Clone)]
pub(super) struct SubtitleTarget {
    pub(super) path: PathBuf,
    pub(super) sort_key: String,
}

pub(super) struct SelectedOutputs {
    pub(super) primary: OutputPath,
    pub(super) extras: Vec<OutputPath>,
}

pub(super) struct OutputPath {
    pub(super) dir: PathBuf,
    pub(super) file_base: String,
}

pub(super) fn select_output_paths(
    output_paths: Vec<PathBuf>,
    template: &str,
    strategy: MultiFolderStrategy,
) -> Result<SelectedOutputs, AppError> {
    let mut output_iter = output_paths.into_iter();
    let primary = output_iter.next().ok_or_else(|| AppError::TemplateEmpty {
        template: template.to_string(),
    })?;
    let primary = split_output_path(&primary, template)?;
    let extras = if matches!(strategy, MultiFolderStrategy::First) {
        Vec::new()
    } else {
        output_iter
            .map(|path| split_output_path(&path, template))
            .collect::<Result<Vec<_>, AppError>>()?
    };
    Ok(SelectedOutputs { primary, extras })
}

fn split_output_path(path: &Path, template: &str) -> Result<OutputPath, AppError> {
    let file_base = path
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::TemplateInvalid {
            template: template.to_string(),
            reason: format!("output path {path:?} is missing a filename"),
        })?
        .to_string();
    let dir = match path.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent.to_path_buf(),
        _ => PathBuf::new(),
    };
    Ok(OutputPath { dir, file_base })
}

pub(super) async fn ensure_output_dir(dir: &Path) -> Result<(), AppError> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(dir)
        .await
        .map_err(|err| AppError::OutputDirCreate {
            path: dir.to_path_buf(),
            source: err,
        })
}

pub(super) async fn ensure_input_file(input: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(input)
        .await
        .map_err(|err| AppError::InputMetadata {
            path: input.to_path_buf(),
            source: err,
        })?;
    if !metadata.is_file() {
        return Err(AppError::InputNotFile {
            path: input.to_path_buf(),
        });
    }
    Ok(())
}

pub(super) fn path_to_string(path: &Path) -> Result<String, AppError> {
    path.to_str()
        .map(|value| value.to_string())
        .ok_or_else(|| AppError::PathNotUtf8 {
            path: path.to_path_buf(),
        })
}

fn extension_string(path: &Path) -> Result<Option<String>, AppError> {
    match path.extension() {
        Some(value) => value
            .to_str()
            .map(|ext| Some(ext.to_string()))
            .ok_or_else(|| AppError::PathNotUtf8 {
                path: path.to_path_buf(),
            }),
        None => Ok(None),
    }
}

async fn find_subtitles(input: &Path, input_stem: &str) -> Result<Vec<SubtitleMatch>, AppError> {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let mut entries = fs::read_dir(parent)
        .await
        .map_err(|err| AppError::SubtitleScan {
            path: parent.to_path_buf(),
            source: err,
        })?;

    let mut matches = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| AppError::SubtitleScan {
            path: parent.to_path_buf(),
            source: err,
        })?
    {
        let path = entry.path();
        if path == input {
            continue;
        }
        if !is_subtitle_path(&path) {
            continue;
        }
        let stem = match path.file_stem().and_then(|value| value.to_str()) {
            Some(value) => value,
            None => {
                return Err(AppError::PathStemNotUtf8 {
                    path: path.to_path_buf(),
                });
            }
        };
        if let Some(suffix) = subtitle_suffix(input_stem, stem) {
            matches.push(SubtitleMatch { path, suffix });
        }
    }

    matches.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(matches)
}

fn is_subtitle_path(path: &Path) -> bool {
    let extension = match path.extension().and_then(|value| value.to_str()) {
        Some(value) => value,
        None => return false,
    };
    let extension = extension.to_ascii_lowercase();
    SUBTITLE_EXTENSIONS
        .iter()
        .any(|candidate| *candidate == extension)
}

pub(super) fn build_output_targets(
    output: &OutputPath,
    parts: &[VideoInputPart],
    subtitle_plans: &[SubtitlePlan],
    local_images: &[String],
) -> Result<OutputTargets, AppError> {
    let mut videos = Vec::with_capacity(parts.len());
    for part in parts {
        let input_extension = extension_string(&part.path)?;
        let part_base = format!("{}{}", output.file_base, part.output_suffix);
        videos.push(
            output
                .dir
                .join(build_file_name(&part_base, input_extension.as_deref())),
        );
    }

    let subtitles = subtitle_plans
        .iter()
        .map(|plan| {
            let extension = extension_string(&plan.path)?;
            let target_base = format!("{}{}", output.file_base, plan.target_base);
            let target_name = build_file_name(&target_base, extension.as_deref());
            Ok(SubtitleTarget {
                path: output.dir.join(target_name),
                sort_key: plan.sort_key.clone(),
            })
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let nfo = output.dir.join(format!("{}.nfo", output.file_base));
    let images = local_images
        .iter()
        .map(|file_name| output.dir.join(file_name))
        .collect::<Vec<_>>();
    Ok(OutputTargets {
        videos,
        nfo,
        subtitles,
        images,
    })
}

pub(super) async fn preflight_moves(moves: &[MoveItem]) -> Result<(), AppError> {
    for item in moves {
        if item.from == item.to {
            continue;
        }
        if path_exists(&item.to).await? {
            return Err(AppError::OutputPathExists {
                path: item.to.clone(),
            });
        }
    }
    Ok(())
}

pub(super) async fn preflight_output_paths(paths: &[PathBuf]) -> Result<(), AppError> {
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        if path_exists(path).await? {
            return Err(AppError::OutputPathExists { path: path.clone() });
        }
    }
    Ok(())
}

pub(super) async fn create_links(
    strategy: MultiFolderStrategy,
    primary: &OutputTargets,
    targets: &OutputTargets,
) -> Result<(), AppError> {
    if matches!(strategy, MultiFolderStrategy::First) {
        return Ok(());
    }
    assert_eq!(
        primary.videos.len(),
        targets.videos.len(),
        "video target length mismatch"
    );
    for (from, to) in primary.videos.iter().zip(targets.videos.iter()) {
        create_link(strategy, from, to).await?;
    }
    assert_eq!(
        primary.subtitles.len(),
        targets.subtitles.len(),
        "subtitle target length mismatch"
    );
    for (from, to) in primary.subtitles.iter().zip(targets.subtitles.iter()) {
        assert_eq!(from.sort_key, to.sort_key, "subtitle link order mismatch");
        create_link(strategy, &from.path, &to.path).await?;
    }
    assert_eq!(
        primary.images.len(),
        targets.images.len(),
        "image target length mismatch"
    );
    for (from, to) in primary.images.iter().zip(targets.images.iter()) {
        create_link(strategy, from, to).await?;
    }
    create_link(strategy, &primary.nfo, &targets.nfo).await?;
    Ok(())
}

async fn path_exists(path: &Path) -> Result<bool, AppError> {
    fs::try_exists(path)
        .await
        .map_err(|err| AppError::FetchRuntime {
            reason: format!("failed to inspect output path {path:?}: {err}"),
        })
}

pub(super) fn build_moves_for_group(
    group: &VideoInputGroup,
    output: &OutputPath,
    subtitle_plans: &[SubtitlePlan],
) -> Result<Vec<MoveItem>, AppError> {
    let mut moves = Vec::new();

    for part in &group.parts {
        let input_extension = extension_string(&part.path)?;
        let part_base = format!("{}{}", output.file_base, part.output_suffix);
        let video_target = output
            .dir
            .join(build_file_name(&part_base, input_extension.as_deref()));
        moves.push(MoveItem {
            from: part.path.clone(),
            to: video_target,
        });
    }

    for subtitle in subtitle_plans {
        let target_base = format!("{}{}", output.file_base, subtitle.target_base);
        let target = build_subtitle_target_by_base(&subtitle.path, &target_base, &output.dir)?;
        moves.push(MoveItem {
            from: subtitle.path.clone(),
            to: target,
        });
    }

    Ok(moves)
}

pub(super) async fn collect_group_subtitle_plans(
    group: &VideoInputGroup,
) -> Result<Vec<SubtitlePlan>, AppError> {
    let mut plans = Vec::new();
    let mut seen = std::collections::HashSet::new();
    let split_group = group.parts.len() > 1;

    for part in &group.parts {
        let part_base = part.output_suffix.clone();
        let subtitles = find_subtitles(&part.path, &part.input_stem).await?;
        for subtitle in subtitles {
            if split_group && !subtitle_has_split_marker(&subtitle.path)? {
                continue;
            }
            if seen.insert(subtitle.path.clone()) {
                let sort_key = format!("0:{}:{part_base}", subtitle_sort_key(&subtitle.path)?);
                plans.push(SubtitlePlan {
                    path: subtitle.path,
                    target_base: format!("{part_base}{}", subtitle.suffix),
                    sort_key,
                });
            }
        }
    }

    let group_subtitles = find_group_subtitles(group).await?;
    for subtitle in group_subtitles {
        if seen.insert(subtitle.path.clone()) {
            plans.push(SubtitlePlan {
                sort_key: format!("1:{}", subtitle_sort_key(&subtitle.path)?),
                path: subtitle.path,
                target_base: subtitle.suffix,
            });
        }
    }

    plans.sort_by(|left, right| left.sort_key.cmp(&right.sort_key));
    Ok(plans)
}

async fn find_group_subtitles(group: &VideoInputGroup) -> Result<Vec<SubtitleMatch>, AppError> {
    let parent = group
        .primary_path
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let mut entries = fs::read_dir(parent)
        .await
        .map_err(|err| AppError::SubtitleScan {
            path: parent.to_path_buf(),
            source: err,
        })?;

    let mut matches = Vec::new();
    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|err| AppError::SubtitleScan {
            path: parent.to_path_buf(),
            source: err,
        })?
    {
        let path = entry.path();
        if !is_subtitle_path(&path) {
            continue;
        }
        let stem = match path.file_stem().and_then(|value| value.to_str()) {
            Some(value) => value,
            None => {
                return Err(AppError::PathStemNotUtf8 {
                    path: path.to_path_buf(),
                });
            }
        };
        if video_parts::stem_contains_split_marker(stem) {
            continue;
        }
        if let Some(suffix) = subtitle_suffix(&group.input_stem, stem) {
            matches.push(SubtitleMatch { path, suffix });
        }
    }

    matches.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(matches)
}

fn subtitle_sort_key(path: &Path) -> Result<String, AppError> {
    path.file_name()
        .and_then(|value| value.to_str())
        .map(|value| value.to_ascii_lowercase())
        .ok_or_else(|| AppError::PathNotUtf8 {
            path: path.to_path_buf(),
        })
}

fn subtitle_has_split_marker(path: &Path) -> Result<bool, AppError> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or_else(|| AppError::PathStemNotUtf8 {
            path: path.to_path_buf(),
        })?;
    Ok(video_parts::stem_contains_split_marker(stem))
}

fn build_subtitle_target_by_base(
    subtitle_path: &Path,
    target_base: &str,
    target_dir: &Path,
) -> Result<PathBuf, AppError> {
    let extension = extension_string(subtitle_path)?;
    let target_name = build_file_name(target_base, extension.as_deref());
    Ok(target_dir.join(target_name))
}

pub(super) struct MoveItem {
    pub(super) from: PathBuf,
    pub(super) to: PathBuf,
}

struct SubtitleMatch {
    path: PathBuf,
    suffix: String,
}

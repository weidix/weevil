use crate::app::TaskContext;
use crate::errors::AppError;
use crate::image_store::localize_movie_images;
use crate::mode_params::{FileModeParams, MultiFolderStrategy};
use crate::source_runner;
use crate::video_parts::{self, VideoInputGroup, VideoInputPart};
use std::path::{Path, PathBuf};
use tokio::fs;

mod fs_ops;
mod input_name;
mod naming;
mod subtitle_match;

use fs_ops::{create_link, move_locked_file};
use input_name::format_input_name;
use naming::{build_file_name, format_output_paths};
pub(crate) use subtitle_match::subtitle_suffix;

const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];

pub(crate) async fn run_file_mode_inputs(
    inputs: &[PathBuf],
    params: &FileModeParams,
) -> Result<(), AppError> {
    let groups = video_parts::group_video_inputs(inputs)?;
    if groups.is_empty() {
        return Err(AppError::FetchRuntime {
            reason: "file mode requires at least one input path".to_string(),
        });
    }

    for group in &groups {
        run_file_mode_group(group, params).await?;
    }
    Ok(())
}

pub(crate) async fn run_file_mode_group(
    group: &VideoInputGroup,
    params: &FileModeParams,
) -> Result<(), AppError> {
    for part in &group.parts {
        ensure_input_file(&part.path).await?;
    }
    let input_name = format_input_name(&group.input_stem, params.input_name_rules())?;
    let input_path = path_to_string(&group.primary_path)?;

    let task = TaskContext::new("file");
    let source_output = source_runner::run_file_scripts(
        &task.id,
        task.kind,
        params.scripts(),
        params.multi_source(),
        params.multi_source_max_sources(),
        params.source_priority(),
        params.node_value_mapper(),
        input_name.as_str(),
        input_path.as_str(),
    )
    .await?;
    let mut movie = source_output.movie;

    let output_paths = format_output_paths(params.output_template(), &movie, &group.input_stem)?;
    let selected = select_output_paths(
        output_paths,
        params.output_template(),
        params.folder_multi(),
    )?;
    let primary_output = selected.primary;
    let extra_outputs = selected.extras;

    ensure_output_dir(&primary_output.dir).await?;
    for output in &extra_outputs {
        ensure_output_dir(&output.dir).await?;
    }

    let (localized_images, xml_output) = if params.save_images() {
        let localized_images = localize_movie_images(
            &mut movie,
            &primary_output.dir,
            &primary_output.file_base,
            &source_output.trusted_urls,
        )
        .await?;
        let xml_output = if localized_images.is_empty() && !source_output.merged_sources {
            source_output.xml
        } else {
            source_runner::serialize_movie(&movie)?
        };
        (localized_images, xml_output)
    } else {
        let xml_output = if source_output.merged_sources {
            source_runner::serialize_movie(&movie)?
        } else {
            source_output.xml
        };
        (Vec::new(), xml_output)
    };

    let subtitle_plans = collect_group_subtitle_plans(group).await?;

    let primary_targets = build_output_targets(
        &primary_output,
        &group.parts,
        &subtitle_plans,
        &localized_images,
    )?;
    let extra_targets = extra_outputs
        .iter()
        .map(|output| {
            build_output_targets(output, &group.parts, &subtitle_plans, &localized_images)
        })
        .collect::<Result<Vec<_>, AppError>>()?;

    let moves = build_moves_for_group(group, &primary_output, &subtitle_plans)?;

    preflight_moves(&moves).await?;
    let mut link_targets = Vec::with_capacity(
        1 + extra_targets.len() * (2 + group.parts.len() + localized_images.len()),
    );
    link_targets.push(primary_targets.nfo.clone());
    for targets in &extra_targets {
        link_targets.extend(targets.videos.iter().cloned());
        link_targets.push(targets.nfo.clone());
        link_targets.extend(targets.subtitles.iter().map(|item| item.path.clone()));
        link_targets.extend(targets.images.iter().cloned());
    }
    preflight_output_paths(&link_targets).await?;

    for item in moves {
        move_locked_file(&item.from, &item.to).await?;
    }

    fs::write(&primary_targets.nfo, xml_output)
        .await
        .map_err(|err| AppError::OutputWrite {
            path: primary_targets.nfo.clone(),
            source: err,
        })?;

    if matches!(
        params.folder_multi(),
        MultiFolderStrategy::HardLink | MultiFolderStrategy::SoftLink
    ) {
        for targets in &extra_targets {
            create_links(params.folder_multi(), &primary_targets, targets).await?;
        }
    }

    Ok(())
}

struct OutputTargets {
    videos: Vec<PathBuf>,
    nfo: PathBuf,
    subtitles: Vec<SubtitleTarget>,
    images: Vec<PathBuf>,
}

#[derive(Debug, Clone)]
struct SubtitlePlan {
    path: PathBuf,
    target_base: String,
    sort_key: String,
}

#[derive(Debug, Clone)]
struct SubtitleTarget {
    path: PathBuf,
    sort_key: String,
}

struct SelectedOutputs {
    primary: OutputPath,
    extras: Vec<OutputPath>,
}

struct OutputPath {
    dir: PathBuf,
    file_base: String,
}

fn select_output_paths(
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

async fn ensure_output_dir(dir: &Path) -> Result<(), AppError> {
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
async fn ensure_input_file(input: &Path) -> Result<(), AppError> {
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
fn path_to_string(path: &Path) -> Result<String, AppError> {
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

fn build_output_targets(
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

async fn preflight_moves(moves: &[MoveItem]) -> Result<(), AppError> {
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

async fn preflight_output_paths(paths: &[PathBuf]) -> Result<(), AppError> {
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

async fn create_links(
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

fn build_moves_for_group(
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

async fn collect_group_subtitle_plans(
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

struct MoveItem {
    from: PathBuf,
    to: PathBuf,
}

struct SubtitleMatch {
    path: PathBuf,
    suffix: String,
}

#[cfg(test)]
#[path = "tests/file_mode.rs"]
mod tests;

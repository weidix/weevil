use crate::app::{TaskContext, render_nfo_output};
use crate::errors::{AppError, LinkKind};
use crate::nfo::Movie;
use fs2::FileExt;
use quick_xml::de::from_str;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use weevil_lua::LuaPlugin;

mod naming;
mod subtitle_match;

use naming::{build_file_name, format_input_name, format_output_paths};
pub(crate) use subtitle_match::subtitle_suffix;

const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];
pub(crate) fn run_file_mode(
    input: &Path,
    script: &Path,
    output_template: &str,
    input_name_remove: &[String],
    folder_multi: MultiFolderStrategy,
) -> Result<(), AppError> {
    ensure_input_file(input)?;
    let input_stem = file_stem_string(input)?;
    let input_name = format_input_name(&input_stem, input_name_remove)?;
    let input_path = path_to_string(input)?;

    let task = TaskContext::new("file");
    let plugin = LuaPlugin::from_file(script).map_err(AppError::LuaPlugin)?;
    plugin.set_log_context(task.id.clone(), task.kind);
    let value = plugin
        .call((input_name.as_str(), input_path.as_str()))
        .map_err(AppError::LuaPlugin)?;
    let xml = render_nfo_output(value, plugin.lua())?;
    let movie: Movie = from_str(&xml).map_err(AppError::NfoParse)?;

    let output_paths = format_output_paths(output_template, &movie, &input_stem)?;
    let selected = select_output_paths(output_paths, output_template, folder_multi)?;
    let primary_output = selected.primary;
    let extra_outputs = selected.extras;

    ensure_output_dir(&primary_output.dir)?;
    for output in &extra_outputs {
        ensure_output_dir(&output.dir)?;
    }

    let input_extension = extension_string(input)?;
    let subtitles = find_subtitles(input, &input_stem)?;
    let primary_targets = build_output_targets(&primary_output, &input_extension, &subtitles)?;
    let extra_targets = extra_outputs
        .iter()
        .map(|output| build_output_targets(output, &input_extension, &subtitles))
        .collect::<Result<Vec<_>, AppError>>()?;

    let subtitle_moves = subtitles
        .iter()
        .map(|subtitle| {
            build_subtitle_move(subtitle, &primary_output.file_base, &primary_output.dir)
        })
        .collect::<Result<Vec<MoveItem>, AppError>>()?;

    let mut moves = Vec::with_capacity(1 + subtitle_moves.len());
    moves.push(MoveItem {
        from: input.to_path_buf(),
        to: primary_targets.video.clone(),
    });
    moves.extend(subtitle_moves);

    preflight_moves(&moves)?;
    let mut link_targets = Vec::with_capacity(1 + extra_targets.len() * 3);
    link_targets.push(primary_targets.nfo.clone());
    for targets in &extra_targets {
        link_targets.push(targets.video.clone());
        link_targets.push(targets.nfo.clone());
        link_targets.extend(targets.subtitles.iter().cloned());
    }
    preflight_output_paths(&link_targets)?;

    for item in moves {
        move_locked_file(&item.from, &item.to)?;
    }

    fs::write(&primary_targets.nfo, xml).map_err(|err| AppError::OutputWrite {
        path: primary_targets.nfo.clone(),
        source: err,
    })?;

    if matches!(
        folder_multi,
        MultiFolderStrategy::HardLink | MultiFolderStrategy::SoftLink
    ) {
        for targets in &extra_targets {
            create_links(folder_multi, &primary_targets, targets)?;
        }
    }

    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MultiFolderStrategy {
    HardLink,
    SoftLink,
    First,
}

struct OutputTargets {
    video: PathBuf,
    nfo: PathBuf,
    subtitles: Vec<PathBuf>,
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

fn ensure_output_dir(dir: &Path) -> Result<(), AppError> {
    if dir.as_os_str().is_empty() {
        return Ok(());
    }
    fs::create_dir_all(dir).map_err(|err| AppError::OutputDirCreate {
        path: dir.to_path_buf(),
        source: err,
    })
}
fn ensure_input_file(input: &Path) -> Result<(), AppError> {
    let metadata = fs::metadata(input).map_err(|err| AppError::InputMetadata {
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
fn file_stem_string(path: &Path) -> Result<String, AppError> {
    path.file_stem()
        .and_then(|value| value.to_str())
        .map(|value| value.to_string())
        .ok_or_else(|| AppError::PathStemNotUtf8 {
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

fn find_subtitles(input: &Path, input_stem: &str) -> Result<Vec<SubtitleMatch>, AppError> {
    let parent = input.parent().unwrap_or_else(|| Path::new("."));
    let entries = fs::read_dir(parent).map_err(|err| AppError::SubtitleScan {
        path: parent.to_path_buf(),
        source: err,
    })?;

    let mut matches = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|err| AppError::SubtitleScan {
            path: parent.to_path_buf(),
            source: err,
        })?;
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

fn build_subtitle_move(
    subtitle: &SubtitleMatch,
    file_base: &str,
    target_dir: &Path,
) -> Result<MoveItem, AppError> {
    let target = build_subtitle_target(subtitle, file_base, target_dir)?;
    Ok(MoveItem {
        from: subtitle.path.clone(),
        to: target,
    })
}

fn build_subtitle_target(
    subtitle: &SubtitleMatch,
    file_base: &str,
    target_dir: &Path,
) -> Result<PathBuf, AppError> {
    let extension = extension_string(&subtitle.path)?;
    let target_base = format!("{file_base}{}", subtitle.suffix);
    let target_name = build_file_name(&target_base, extension.as_deref());
    Ok(target_dir.join(target_name))
}

fn build_output_targets(
    output: &OutputPath,
    input_extension: &Option<String>,
    subtitles: &[SubtitleMatch],
) -> Result<OutputTargets, AppError> {
    let video = output.dir.join(build_file_name(
        &output.file_base,
        input_extension.as_deref(),
    ));
    let nfo = output.dir.join(format!("{}.nfo", output.file_base));
    let mut subtitle_targets = Vec::with_capacity(subtitles.len());
    for subtitle in subtitles {
        subtitle_targets.push(build_subtitle_target(
            subtitle,
            &output.file_base,
            &output.dir,
        )?);
    }
    Ok(OutputTargets {
        video,
        nfo,
        subtitles: subtitle_targets,
    })
}

fn preflight_moves(moves: &[MoveItem]) -> Result<(), AppError> {
    for item in moves {
        if item.from == item.to {
            continue;
        }
        if item.to.exists() {
            return Err(AppError::OutputPathExists {
                path: item.to.clone(),
            });
        }
    }
    Ok(())
}

fn preflight_output_paths(paths: &[PathBuf]) -> Result<(), AppError> {
    let mut seen = std::collections::HashSet::new();
    for path in paths {
        if !seen.insert(path.clone()) {
            continue;
        }
        if path.exists() {
            return Err(AppError::OutputPathExists { path: path.clone() });
        }
    }
    Ok(())
}

fn create_links(
    strategy: MultiFolderStrategy,
    primary: &OutputTargets,
    targets: &OutputTargets,
) -> Result<(), AppError> {
    if matches!(strategy, MultiFolderStrategy::First) {
        return Ok(());
    }
    create_link(strategy, &primary.video, &targets.video)?;
    assert_eq!(
        primary.subtitles.len(),
        targets.subtitles.len(),
        "subtitle target length mismatch"
    );
    for (from, to) in primary.subtitles.iter().zip(targets.subtitles.iter()) {
        create_link(strategy, from, to)?;
    }
    create_link(strategy, &primary.nfo, &targets.nfo)?;
    Ok(())
}

fn create_link(strategy: MultiFolderStrategy, from: &Path, to: &Path) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    match strategy {
        MultiFolderStrategy::HardLink => {
            fs::hard_link(from, to).map_err(|err| AppError::FileLink {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                kind: LinkKind::Hard,
                source: err,
            })
        }
        MultiFolderStrategy::SoftLink => {
            create_soft_link(from, to).map_err(|err| AppError::FileLink {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                kind: LinkKind::Soft,
                source: err,
            })
        }
        MultiFolderStrategy::First => Ok(()),
    }
}

fn create_soft_link(from: &Path, to: &Path) -> Result<(), std::io::Error> {
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(from, to)
    }
    #[cfg(windows)]
    {
        std::os::windows::fs::symlink_file(from, to)
    }
    #[cfg(not(any(unix, windows)))]
    {
        Err(std::io::Error::new(
            std::io::ErrorKind::Other,
            "soft links are not supported on this platform",
        ))
    }
}

fn move_locked_file(from: &Path, to: &Path) -> Result<(), AppError> {
    if from == to {
        return Ok(());
    }
    let file = lock_exclusive(from)?;
    match fs::rename(from, to) {
        Ok(()) => Ok(()),
        Err(err) if is_cross_device_error(&err) => {
            fs::copy(from, to).map_err(|copy_err| AppError::FileCopy {
                from: from.to_path_buf(),
                to: to.to_path_buf(),
                source: copy_err,
            })?;
            drop(file);
            fs::remove_file(from).map_err(|remove_err| AppError::FileRemove {
                path: from.to_path_buf(),
                source: remove_err,
            })?;
            Ok(())
        }
        Err(err) => Err(AppError::FileMove {
            from: from.to_path_buf(),
            to: to.to_path_buf(),
            source: err,
        }),
    }
}

fn is_cross_device_error(error: &std::io::Error) -> bool {
    error.raw_os_error() == Some(18)
}

fn lock_exclusive(path: &Path) -> Result<File, AppError> {
    let file = File::open(path).map_err(|err| AppError::FileLock {
        path: path.to_path_buf(),
        source: err,
    })?;
    file.lock_exclusive().map_err(|err| AppError::FileLock {
        path: path.to_path_buf(),
        source: err,
    })?;
    Ok(file)
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

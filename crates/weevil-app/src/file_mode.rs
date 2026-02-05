use crate::app::{TaskContext, render_nfo_output};
use crate::errors::AppError;
use crate::nfo::Movie;
use fs2::FileExt;
use quick_xml::de::from_str;
use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use weevil_lua::LuaPlugin;

mod naming;

use naming::{build_file_name, format_file_base, format_folder_path, format_input_name};

const DEFAULT_FILE_TEMPLATE: &str = "{title}";
const DEFAULT_FOLDER_TEMPLATE: &str = "{title}";
const SUBTITLE_EXTENSIONS: &[&str] = &["srt", "ass", "ssa", "vtt", "sub", "idx", "sup"];
pub(crate) fn run_file_mode(
    input: &Path,
    script: &Path,
    output_dir: &Path,
    input_name_remove: &[String],
    file_format: &str,
    folder_format: &str,
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

    let file_template = if file_format.trim().is_empty() {
        DEFAULT_FILE_TEMPLATE
    } else {
        file_format
    };
    let folder_template = if folder_format.trim().is_empty() {
        DEFAULT_FOLDER_TEMPLATE
    } else {
        folder_format
    };

    let file_base = format_file_base(file_template, &movie, &input_stem)?;
    let folder_path = format_folder_path(folder_template, &movie, &input_stem)?;

    fs::create_dir_all(output_dir).map_err(|err| AppError::OutputDirCreate {
        path: output_dir.to_path_buf(),
        source: err,
    })?;

    let target_dir = output_dir.join(folder_path);
    fs::create_dir_all(&target_dir).map_err(|err| AppError::OutputDirCreate {
        path: target_dir.clone(),
        source: err,
    })?;

    let input_extension = extension_string(input)?;
    let video_target = target_dir.join(build_file_name(&file_base, input_extension.as_deref()));
    let nfo_target = target_dir.join(format!("{file_base}.nfo"));

    let subtitles = find_subtitles(input, &input_stem)?;
    let subtitle_moves = subtitles
        .into_iter()
        .map(|subtitle| build_subtitle_move(&subtitle, &file_base, &target_dir))
        .collect::<Result<Vec<MoveItem>, AppError>>()?;

    let mut moves = Vec::with_capacity(1 + subtitle_moves.len());
    moves.push(MoveItem {
        from: input.to_path_buf(),
        to: video_target.clone(),
    });
    moves.extend(subtitle_moves);

    preflight_moves(&moves)?;
    if nfo_target.exists() {
        return Err(AppError::OutputPathExists { path: nfo_target });
    }

    for item in moves {
        move_locked_file(&item.from, &item.to)?;
    }

    fs::write(&nfo_target, xml).map_err(|err| AppError::OutputWrite {
        path: nfo_target,
        source: err,
    })?;

    Ok(())
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

fn subtitle_suffix(video_stem: &str, subtitle_stem: &str) -> Option<String> {
    if subtitle_stem == video_stem {
        return Some(String::new());
    }
    subtitle_stem
        .strip_prefix(video_stem)
        .and_then(|rest| rest.strip_prefix('.'))
        .map(|rest| format!(".{rest}"))
}

fn build_subtitle_move(
    subtitle: &SubtitleMatch,
    file_base: &str,
    target_dir: &Path,
) -> Result<MoveItem, AppError> {
    let extension = extension_string(&subtitle.path)?;
    let target_base = format!("{file_base}{}", subtitle.suffix);
    let target_name = build_file_name(&target_base, extension.as_deref());
    let target = target_dir.join(target_name);
    Ok(MoveItem {
        from: subtitle.path.clone(),
        to: target,
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

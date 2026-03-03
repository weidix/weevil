use crate::app::TaskContext;
use crate::errors::AppError;
use crate::image_store::localize_movie_images;
use crate::mode_params::{FileModeParams, MultiFolderStrategy};
use crate::script_throttle::ScriptThrottleConfig;
use crate::source_runner;
use crate::video_parts::{self, VideoInputGroup};
use std::path::PathBuf;
use tokio::fs;

mod fs_ops;
mod input_name;
mod naming;
mod plan;
mod subtitle_match;

use fs_ops::move_locked_file;
use input_name::format_input_name;
use naming::format_output_paths;
#[cfg(test)]
use plan::{OutputPath, SubtitlePlan};
use plan::{
    build_moves_for_group, build_output_targets, collect_group_subtitle_plans, create_links,
    ensure_input_file, ensure_output_dir, path_to_string, preflight_moves, preflight_output_paths,
    select_output_paths,
};
#[cfg(test)]
pub(crate) use subtitle_match::subtitle_suffix;

pub(crate) async fn run_file_mode_inputs(
    inputs: &[PathBuf],
    params: &FileModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Result<(), AppError> {
    let groups = video_parts::group_video_inputs(inputs)?;
    if groups.is_empty() {
        return Err(AppError::FetchRuntime {
            reason: "file mode requires at least one input path".to_string(),
        });
    }

    for group in &groups {
        run_file_mode_group(group, params, script_throttle).await?;
    }
    Ok(())
}

pub(crate) async fn run_file_mode_group(
    group: &VideoInputGroup,
    params: &FileModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Result<(), AppError> {
    for part in &group.parts {
        ensure_input_file(&part.path).await?;
    }
    let input_name = format_input_name(&group.input_stem, params.input_name_rules())?;
    let input_path = path_to_string(&group.primary_path)?;

    let task = TaskContext::new("file");
    let run_config = source_runner::ScriptRunConfig {
        task_id: &task.id,
        task_kind: task.kind,
        scripts: params.scripts(),
        multi_source: params.multi_source(),
        multi_source_max_sources: params.multi_source_max_sources(),
        source_priority: params.source_priority(),
        mapper: params.node_value_mapper(),
        translator: params.movie_translator(),
        script_throttle,
    };
    let source_output =
        source_runner::run_file_scripts(&run_config, input_name.as_str(), input_path.as_str())
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

#[cfg(test)]
#[path = "tests/file_mode.rs"]
mod tests;

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::task::JoinSet;
use tracing::info;

use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::{FetchModeParams, FileModeParams};
use crate::script_throttle::ScriptThrottleConfig;

pub(crate) type FetchTaskResult = (Vec<PathBuf>, Result<(), String>);

pub(crate) async fn preflight_script(
    fetch: &FetchModeParams,
    script_paths: &[PathBuf],
) -> Result<(), AppError> {
    preflight_script_for_multithread(fetch, script_paths).await
}

pub(crate) async fn run_batch_fetch(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<(), AppError> {
    preflight_script_for_multithread(fetch, params.scripts()).await?;
    let script_throttle = script_throttle_config(fetch);

    if groups.is_empty() {
        return Ok(());
    }

    if fetch.fetch_threads() == 1 {
        return run_serial_for_dir(groups, params, script_throttle).await;
    }

    let results = run_batch_fetch_with_results(groups, params, fetch, script_throttle).await?;
    for (group, result) in results {
        if let Err(reason) = result {
            return Err(AppError::FetchRuntime {
                reason: format!("task {:?} failed: {reason}", group),
            });
        }
    }
    Ok(())
}

async fn run_serial_for_dir(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Result<(), AppError> {
    for group in groups {
        file_mode::run_file_mode_inputs(&group, params, script_throttle).await?;
    }
    Ok(())
}

pub(crate) async fn run_batch_fetch_with_results(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Result<Vec<FetchTaskResult>, AppError> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    if fetch.fetch_threads() == 1 {
        Ok(run_serial(groups, params, script_throttle).await)
    } else {
        run_parallel(groups, params, fetch, script_throttle).await
    }
}

async fn run_serial(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Vec<FetchTaskResult> {
    let execute = |inputs: &[PathBuf]| {
        let inputs = inputs.to_vec();
        async move {
            file_mode::run_file_mode_inputs(&inputs, params, script_throttle)
                .await
                .map_err(|err| err.to_string())
        }
    };
    run_serial_with_executor(groups, &execute).await
}

async fn run_serial_with_executor<F, Fut>(
    groups: Vec<Vec<PathBuf>>,
    execute: &F,
) -> Vec<FetchTaskResult>
where
    F: Fn(&[PathBuf]) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let mut results = Vec::with_capacity(groups.len());
    for group in groups {
        let result = execute(&group).await;
        results.push((group, result));
    }
    results
}

async fn run_parallel(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
    script_throttle: ScriptThrottleConfig,
) -> Result<Vec<FetchTaskResult>, AppError> {
    let params = params.clone();
    let fetch = *fetch;
    let execute = move |inputs: &[PathBuf]| {
        let inputs = inputs.to_vec();
        let params = params.clone();
        let script_throttle = script_throttle;
        async move {
            file_mode::run_file_mode_inputs(&inputs, &params, script_throttle)
                .await
                .map_err(|err| err.to_string())
        }
    };
    run_parallel_with_executor(groups, &fetch, execute).await
}

async fn run_parallel_with_executor<F, Fut>(
    groups: Vec<Vec<PathBuf>>,
    fetch: &FetchModeParams,
    execute: F,
) -> Result<Vec<FetchTaskResult>, AppError>
where
    F: Fn(&[PathBuf]) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    let fetch = *fetch;
    let worker_count = effective_worker_count(fetch.fetch_threads(), groups.len());
    if worker_count <= 1 {
        return Ok(run_serial_with_executor(groups, &execute).await);
    }

    let execute = Arc::new(execute);
    let mut join_set: JoinSet<FetchTaskResult> = JoinSet::new();
    let mut results = Vec::with_capacity(groups.len());
    let mut pending = 0usize;
    let mut stop_spawning = false;
    let mut iter = groups.into_iter();

    for _ in 0..worker_count {
        if let Some(group) = iter.next() {
            spawn_task(&mut join_set, Arc::clone(&execute), group);
            pending += 1;
        }
    }

    while pending > 0 {
        match join_set.join_next().await {
            Some(Ok(result)) => {
                if result.1.is_err() {
                    stop_spawning = true;
                }
                results.push(result);
                pending -= 1;
                if !stop_spawning && let Some(group) = iter.next() {
                    spawn_task(&mut join_set, Arc::clone(&execute), group);
                    pending += 1;
                }
            }
            Some(Err(err)) => {
                return Err(AppError::FetchRuntime {
                    reason: format!("failed to join worker task: {err}"),
                });
            }
            None => break,
        }
    }

    Ok(results)
}

fn spawn_task<F, Fut>(join_set: &mut JoinSet<FetchTaskResult>, execute: Arc<F>, group: Vec<PathBuf>)
where
    F: Fn(&[PathBuf]) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    join_set.spawn(async move {
        let result = execute(&group).await;
        (group, result)
    });
}

fn effective_worker_count(configured: u32, task_count: usize) -> usize {
    if task_count == 0 {
        return 1;
    }
    if configured == 0 {
        return task_count.max(1);
    }
    let configured = usize::try_from(configured).unwrap_or(task_count);
    configured.clamp(1, task_count.max(1))
}

pub(crate) fn script_throttle_config(fetch: &FetchModeParams) -> ScriptThrottleConfig {
    if fetch.throttle_same_script() {
        ScriptThrottleConfig::enabled(fetch.script_throttle_base_ms())
    } else {
        ScriptThrottleConfig::disabled()
    }
}

async fn preflight_script_for_multithread(
    fetch: &FetchModeParams,
    script_paths: &[PathBuf],
) -> Result<(), AppError> {
    if !fetch.multithread_enabled() {
        return Ok(());
    }

    info!(
        "multi-thread preflight passed for scripts {:?} (fetch_threads={}, throttle_same_script={}, script_throttle_base_ms={})",
        script_paths,
        fetch.fetch_threads(),
        fetch.throttle_same_script(),
        fetch.script_throttle_base_ms()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use std::sync::atomic::Ordering;
    use std::time::Duration;
    use tokio::time::sleep;

    #[test]
    fn worker_count_with_zero_is_unlimited() {
        assert_eq!(effective_worker_count(0, 5), 5);
    }

    #[test]
    fn worker_count_respects_upper_bound() {
        assert_eq!(effective_worker_count(8, 3), 3);
    }

    #[test]
    fn worker_count_respects_serial_mode() {
        assert_eq!(effective_worker_count(1, 3), 1);
    }

    #[tokio::test]
    async fn parallel_executor_never_exceeds_configured_limit() {
        let task_count = 20;
        let groups = (0..task_count)
            .map(|index| vec![PathBuf::from(format!("task-{index}.mkv"))])
            .collect::<Vec<_>>();
        let fetch = FetchModeParams::new(3, false, 0);

        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let execute = {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            move |_: &[PathBuf]| {
                let active = Arc::clone(&active);
                let peak = Arc::clone(&peak);
                async move {
                    let current = active.fetch_add(1, Ordering::SeqCst) + 1;

                    loop {
                        let observed = peak.load(Ordering::SeqCst);
                        if current <= observed {
                            break;
                        }
                        if peak
                            .compare_exchange(observed, current, Ordering::SeqCst, Ordering::SeqCst)
                            .is_ok()
                        {
                            break;
                        }
                    }

                    sleep(Duration::from_millis(30)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                }
            }
        };

        let results = run_parallel_with_executor(groups, &fetch, execute)
            .await
            .expect("run parallel");
        assert_eq!(results.len(), task_count);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
        assert!(peak.load(Ordering::SeqCst) <= 3);
        assert!(peak.load(Ordering::SeqCst) >= 2);
    }
}

use std::future::Future;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::sleep;
use tracing::info;

use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::{FetchModeParams, FileModeParams};

const SCRIPT_THROTTLE_SPAN_MS: u64 = 200;

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

    if groups.is_empty() {
        return Ok(());
    }

    if fetch.fetch_threads() == 1 {
        return run_serial_for_dir(
            groups,
            params,
            fetch.throttle_same_script(),
            fetch.script_throttle_base_ms(),
        )
        .await;
    }

    let results = run_batch_fetch_with_results(groups, params, fetch).await?;
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
    throttle_same_script: bool,
    throttle_base_ms: u64,
) -> Result<(), AppError> {
    let mut has_previous = false;
    for group in groups {
        if throttle_same_script && has_previous {
            throttle_script_execution(throttle_base_ms).await;
        }
        file_mode::run_file_mode_inputs(&group, params).await?;
        has_previous = true;
    }
    Ok(())
}

pub(crate) async fn run_batch_fetch_with_results(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<Vec<FetchTaskResult>, AppError> {
    if groups.is_empty() {
        return Ok(Vec::new());
    }

    if fetch.fetch_threads() == 1 {
        Ok(run_serial(
            groups,
            params,
            fetch.throttle_same_script(),
            fetch.script_throttle_base_ms(),
        )
        .await)
    } else {
        run_parallel(groups, params, fetch).await
    }
}

async fn run_serial(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    throttle_same_script: bool,
    throttle_base_ms: u64,
) -> Vec<FetchTaskResult> {
    let execute = |inputs: &[PathBuf]| {
        let inputs = inputs.to_vec();
        async move {
            file_mode::run_file_mode_inputs(&inputs, params)
                .await
                .map_err(|err| err.to_string())
        }
    };
    run_serial_with_executor(groups, throttle_same_script, throttle_base_ms, &execute).await
}

async fn run_serial_with_executor<F, Fut>(
    groups: Vec<Vec<PathBuf>>,
    throttle_same_script: bool,
    throttle_base_ms: u64,
    execute: &F,
) -> Vec<FetchTaskResult>
where
    F: Fn(&[PathBuf]) -> Fut,
    Fut: Future<Output = Result<(), String>>,
{
    let mut results = Vec::with_capacity(groups.len());
    let mut has_previous = false;
    for group in groups {
        if throttle_same_script && has_previous {
            throttle_script_execution(throttle_base_ms).await;
        }
        let result = execute(&group).await;
        results.push((group, result));
        has_previous = true;
    }
    results
}

async fn run_parallel(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<Vec<FetchTaskResult>, AppError> {
    let params = params.clone();
    let fetch = *fetch;
    let execute = move |inputs: &[PathBuf]| {
        let inputs = inputs.to_vec();
        let params = params.clone();
        async move {
            file_mode::run_file_mode_inputs(&inputs, &params)
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
        return Ok(run_serial_with_executor(
            groups,
            fetch.throttle_same_script(),
            fetch.script_throttle_base_ms(),
            &execute,
        )
        .await);
    }

    let execute = Arc::new(execute);
    let throttle = if fetch.throttle_same_script() {
        Some(Arc::new(ScriptThrottle::new()))
    } else {
        None
    };
    let throttle_base_ms = fetch.script_throttle_base_ms();
    let mut join_set: JoinSet<FetchTaskResult> = JoinSet::new();
    let mut results = Vec::with_capacity(groups.len());
    let mut pending = 0usize;
    let mut stop_spawning = false;
    let mut iter = groups.into_iter();

    for _ in 0..worker_count {
        if let Some(group) = iter.next() {
            spawn_task(
                &mut join_set,
                Arc::clone(&execute),
                throttle.clone(),
                throttle_base_ms,
                group,
            );
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
                if !stop_spawning {
                    if let Some(group) = iter.next() {
                        spawn_task(
                            &mut join_set,
                            Arc::clone(&execute),
                            throttle.clone(),
                            throttle_base_ms,
                            group,
                        );
                        pending += 1;
                    }
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

fn spawn_task<F, Fut>(
    join_set: &mut JoinSet<FetchTaskResult>,
    execute: Arc<F>,
    throttle: Option<Arc<ScriptThrottle>>,
    throttle_base_ms: u64,
    group: Vec<PathBuf>,
) where
    F: Fn(&[PathBuf]) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<(), String>> + Send + 'static,
{
    join_set.spawn(async move {
        let result = if let Some(throttle) = throttle {
            let permit = match Arc::clone(&throttle.gate).acquire_owned().await {
                Ok(permit) => permit,
                Err(_) => {
                    return (group, Err("script throttle gate closed".to_string()));
                }
            };
            if throttle.has_previous.load(Ordering::Relaxed) {
                throttle_script_execution(throttle_base_ms).await;
            }
            let result = execute(&group).await;
            throttle.has_previous.store(true, Ordering::Relaxed);
            drop(permit);
            result
        } else {
            execute(&group).await
        };
        (group, result)
    });
}

#[derive(Debug)]
struct ScriptThrottle {
    gate: Arc<Semaphore>,
    has_previous: AtomicBool,
}

impl ScriptThrottle {
    fn new() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(1)),
            has_previous: AtomicBool::new(false),
        }
    }
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

async fn throttle_script_execution(base_ms: u64) {
    let delay_ms = random_script_delay_ms(base_ms);
    sleep(Duration::from_millis(delay_ms)).await;
}

fn random_script_delay_ms(base_ms: u64) -> u64 {
    if base_ms == 0 {
        return 0;
    }

    let now_nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let span = u128::from(SCRIPT_THROTTLE_SPAN_MS) + 1;
    let centered_offset =
        i128::try_from(now_nanos % span).unwrap_or(0) - i128::from(SCRIPT_THROTTLE_SPAN_MS / 2);

    if base_ms < 100 {
        let delay = i128::from(base_ms) + centered_offset;
        return u64::try_from(delay.abs()).unwrap_or(0);
    }

    let delay = i128::from(base_ms) + centered_offset;
    if delay <= 0 {
        0
    } else {
        u64::try_from(delay).unwrap_or(base_ms)
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

    #[test]
    fn random_delay_stays_in_expected_range() {
        for _ in 0..32 {
            let delay = random_script_delay_ms(1000);
            assert!((900..=1100).contains(&delay));
        }
    }

    #[test]
    fn random_delay_follows_custom_base_ms() {
        for _ in 0..32 {
            let delay = random_script_delay_ms(2500);
            assert!((2400..=2600).contains(&delay));
        }
    }

    #[test]
    fn random_delay_is_disabled_when_base_is_zero() {
        for _ in 0..32 {
            let delay = random_script_delay_ms(0);
            assert_eq!(delay, 0);
        }
    }

    #[test]
    fn random_delay_uses_abs_when_base_below_100() {
        for _ in 0..128 {
            let delay = random_script_delay_ms(1);
            assert!(delay <= 101);
        }
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

    #[tokio::test]
    async fn throttle_same_script_prevents_parallel_script_execution() {
        let task_count = 16;
        let groups = (0..task_count)
            .map(|index| vec![PathBuf::from(format!("throttle-task-{index}.mkv"))])
            .collect::<Vec<_>>();
        let fetch = FetchModeParams::new(4, true, 0);

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

                    sleep(Duration::from_millis(20)).await;
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
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }
}

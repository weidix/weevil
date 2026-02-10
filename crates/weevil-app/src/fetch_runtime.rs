use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tracing::info;

use crate::errors::AppError;
use crate::file_mode;
use crate::mode_params::{FetchModeParams, FileModeParams};

const SCRIPT_THROTTLE_SPAN_MS: u64 = 200;

#[derive(Debug)]
struct ScriptThrottleState {
    has_previous_task: bool,
}

impl ScriptThrottleState {
    fn new() -> Self {
        Self {
            has_previous_task: false,
        }
    }
}

pub(crate) type FetchTaskResult = (Vec<PathBuf>, Result<(), String>);

pub(crate) fn preflight_script(
    fetch: &FetchModeParams,
    script_paths: &[PathBuf],
) -> Result<(), AppError> {
    preflight_script_for_multithread(fetch, script_paths)
}
pub(crate) fn run_batch_fetch(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<(), AppError> {
    preflight_script_for_multithread(fetch, params.scripts())?;

    if groups.is_empty() {
        return Ok(());
    }

    if fetch.fetch_threads() == 1 {
        return run_serial_for_dir(
            groups,
            params,
            fetch.throttle_same_script(),
            fetch.script_throttle_base_ms(),
        );
    }

    let results = run_batch_fetch_with_results(groups, params, fetch)?;
    for (group, result) in results {
        if let Err(reason) = result {
            return Err(AppError::FetchRuntime {
                reason: format!("task {:?} failed: {reason}", group),
            });
        }
    }
    Ok(())
}

fn run_serial_for_dir(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    throttle_same_script: bool,
    throttle_base_ms: u64,
) -> Result<(), AppError> {
    let mut has_previous = false;
    for group in groups {
        if throttle_same_script && has_previous {
            throttle_script_execution(throttle_base_ms);
        }
        file_mode::run_file_mode_inputs(&group, params)?;
        has_previous = true;
    }
    Ok(())
}

pub(crate) fn run_batch_fetch_with_results(
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
        ))
    } else {
        run_parallel(groups, params, fetch)
    }
}

fn run_serial(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    throttle_same_script: bool,
    throttle_base_ms: u64,
) -> Vec<FetchTaskResult> {
    let execute = |inputs: &[PathBuf]| {
        file_mode::run_file_mode_inputs(inputs, params).map_err(|err| err.to_string())
    };
    run_serial_with_executor(groups, throttle_same_script, throttle_base_ms, &execute)
}

fn run_serial_with_executor<F>(
    groups: Vec<Vec<PathBuf>>,
    throttle_same_script: bool,
    throttle_base_ms: u64,
    execute: &F,
) -> Vec<FetchTaskResult>
where
    F: Fn(&[PathBuf]) -> Result<(), String>,
{
    let mut results = Vec::with_capacity(groups.len());
    let mut has_previous = false;
    for group in groups {
        if throttle_same_script && has_previous {
            throttle_script_execution(throttle_base_ms);
        }
        let result = execute(&group);
        results.push((group, result));
        has_previous = true;
    }
    results
}

fn run_parallel(
    groups: Vec<Vec<PathBuf>>,
    params: &FileModeParams,
    fetch: &FetchModeParams,
) -> Result<Vec<FetchTaskResult>, AppError> {
    let execute = |inputs: &[PathBuf]| {
        file_mode::run_file_mode_inputs(inputs, params).map_err(|err| err.to_string())
    };
    run_parallel_with_executor(groups, fetch, &execute)
}

fn run_parallel_with_executor<F>(
    groups: Vec<Vec<PathBuf>>,
    fetch: &FetchModeParams,
    execute: &F,
) -> Result<Vec<FetchTaskResult>, AppError>
where
    F: Fn(&[PathBuf]) -> Result<(), String> + Sync,
{
    let worker_count = effective_worker_count(fetch.fetch_threads(), groups.len());
    if worker_count <= 1 {
        return Ok(run_serial_with_executor(
            groups,
            fetch.throttle_same_script(),
            fetch.script_throttle_base_ms(),
            execute,
        ));
    }

    let input_queue = Arc::new(std::sync::Mutex::new(groups.into_iter()));
    let runtime_error = Arc::new(std::sync::Mutex::new(None::<String>));
    let results = Arc::new(std::sync::Mutex::new(Vec::<FetchTaskResult>::new()));
    let stop_flag = Arc::new(AtomicBool::new(false));
    let script_gate = if fetch.throttle_same_script() {
        Some(Arc::new(std::sync::Mutex::new(ScriptThrottleState::new())))
    } else {
        None
    };

    thread::scope(|scope| {
        for _ in 0..worker_count {
            let queue = Arc::clone(&input_queue);
            let error_slot = Arc::clone(&runtime_error);
            let output = Arc::clone(&results);
            let stop = Arc::clone(&stop_flag);
            let throttle_base_ms = fetch.script_throttle_base_ms();
            let gate = script_gate.clone();
            let execute = execute;
            scope.spawn(move || {
                worker_loop(
                    queue,
                    gate,
                    execute,
                    output,
                    error_slot,
                    stop,
                    throttle_base_ms,
                );
            });
        }
    });

    let mut error_guard = runtime_error.lock().map_err(|_| AppError::FetchRuntime {
        reason: "failed to read worker error state".to_string(),
    })?;
    if let Some(reason) = error_guard.take() {
        return Err(AppError::FetchRuntime { reason });
    }

    let mut output = results.lock().map_err(|_| AppError::FetchRuntime {
        reason: "failed to read worker results".to_string(),
    })?;
    Ok(std::mem::take(&mut *output))
}

fn worker_loop(
    queue: Arc<std::sync::Mutex<std::vec::IntoIter<Vec<PathBuf>>>>,
    script_gate: Option<Arc<std::sync::Mutex<ScriptThrottleState>>>,
    execute: &impl Fn(&[PathBuf]) -> Result<(), String>,
    results: Arc<std::sync::Mutex<Vec<FetchTaskResult>>>,
    runtime_error: Arc<std::sync::Mutex<Option<String>>>,
    stop_flag: Arc<AtomicBool>,
    throttle_base_ms: u64,
) {
    loop {
        if stop_flag.load(Ordering::Relaxed) {
            return;
        }

        let next_file = {
            let mut queue = match queue.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    store_runtime_error(
                        &runtime_error,
                        "task queue lock poisoned".to_string(),
                        &stop_flag,
                    );
                    return;
                }
            };
            queue.next()
        };

        let Some(group) = next_file else {
            return;
        };

        let result = if let Some(gate) = &script_gate {
            let mut state = match gate.lock() {
                Ok(guard) => guard,
                Err(_) => {
                    store_runtime_error(
                        &runtime_error,
                        "script throttle lock poisoned".to_string(),
                        &stop_flag,
                    );
                    return;
                }
            };
            if state.has_previous_task {
                throttle_script_execution(throttle_base_ms);
            }
            let result = execute(&group);
            state.has_previous_task = true;
            result
        } else {
            execute(&group)
        };

        let has_error = result.is_err();
        if push_task_result(&results, (group, result)).is_err() {
            store_runtime_error(
                &runtime_error,
                "failed to write task result".to_string(),
                &stop_flag,
            );
            return;
        }

        if has_error {
            stop_flag.store(true, Ordering::Relaxed);
            return;
        }
    }
}

fn push_task_result(
    results: &Arc<std::sync::Mutex<Vec<FetchTaskResult>>>,
    item: FetchTaskResult,
) -> Result<(), ()> {
    let mut guard = results.lock().map_err(|_| ())?;
    guard.push(item);
    Ok(())
}

fn store_runtime_error(
    slot: &Arc<std::sync::Mutex<Option<String>>>,
    reason: String,
    stop_flag: &Arc<AtomicBool>,
) {
    if let Ok(mut guard) = slot.lock() {
        if guard.is_none() {
            *guard = Some(reason);
        }
    }
    stop_flag.store(true, Ordering::Relaxed);
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

fn throttle_script_execution(base_ms: u64) {
    let delay_ms = random_script_delay_ms(base_ms);
    thread::sleep(Duration::from_millis(delay_ms));
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

fn preflight_script_for_multithread(
    fetch: &FetchModeParams,
    script_paths: &[PathBuf],
) -> Result<(), AppError> {
    if !fetch.multithread_enabled() {
        return Ok(());
    }

    for script_path in script_paths {
        if !weevil_lua::script_uses_only_async_http_file(script_path)
            .map_err(AppError::LuaPlugin)?
        {
            return Err(AppError::ScriptSyncHttpNotAllowed {
                path: script_path.to_path_buf(),
            });
        }
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

    #[test]
    fn parallel_executor_never_exceeds_configured_limit() {
        let task_count = 20;
        let groups = (0..task_count)
            .map(|index| vec![PathBuf::from(format!("task-{index}.mkv"))])
            .collect::<Vec<_>>();
        let fetch = FetchModeParams::new(3, false, 0);

        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let execute = |_: &[PathBuf]| -> Result<(), String> {
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

            std::thread::sleep(Duration::from_millis(30));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        };

        let results = run_parallel_with_executor(groups, &fetch, &execute).expect("run parallel");
        assert_eq!(results.len(), task_count);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
        assert!(peak.load(Ordering::SeqCst) <= 3);
        assert!(peak.load(Ordering::SeqCst) >= 2);
    }

    #[test]
    fn throttle_same_script_prevents_parallel_script_execution() {
        let task_count = 16;
        let groups = (0..task_count)
            .map(|index| vec![PathBuf::from(format!("throttle-task-{index}.mkv"))])
            .collect::<Vec<_>>();
        let fetch = FetchModeParams::new(4, true, 0);

        let active = AtomicUsize::new(0);
        let peak = AtomicUsize::new(0);
        let execute = |_: &[PathBuf]| -> Result<(), String> {
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

            std::thread::sleep(Duration::from_millis(20));
            active.fetch_sub(1, Ordering::SeqCst);
            Ok(())
        };

        let results = run_parallel_with_executor(groups, &fetch, &execute).expect("run parallel");
        assert_eq!(results.len(), task_count);
        assert!(results.iter().all(|(_, result)| result.is_ok()));
        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }
}

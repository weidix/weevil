use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use tokio::sync::{Mutex, Semaphore};
use tokio::time::sleep;

use crate::errors::AppError;

const SCRIPT_THROTTLE_SPAN_MS: u64 = 200;

#[derive(Clone, Copy, Debug)]
pub(crate) struct ScriptThrottleConfig {
    enabled: bool,
    base_ms: u64,
}

impl ScriptThrottleConfig {
    pub(crate) fn enabled(base_ms: u64) -> Self {
        Self {
            enabled: true,
            base_ms,
        }
    }

    pub(crate) fn disabled() -> Self {
        Self {
            enabled: false,
            base_ms: 0,
        }
    }

    pub(crate) fn is_enabled(self) -> bool {
        self.enabled
    }

    pub(crate) fn base_ms(self) -> u64 {
        self.base_ms
    }
}

#[derive(Debug)]
struct ScriptThrottleEntry {
    gate: Arc<Semaphore>,
    has_previous: AtomicBool,
}

impl ScriptThrottleEntry {
    fn new() -> Self {
        Self {
            gate: Arc::new(Semaphore::new(1)),
            has_previous: AtomicBool::new(false),
        }
    }
}

#[derive(Debug)]
struct ScriptThrottleRegistry {
    entries: Mutex<HashMap<String, Arc<ScriptThrottleEntry>>>,
}

impl ScriptThrottleRegistry {
    fn new() -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
        }
    }

    async fn entry(&self, key: &str) -> Arc<ScriptThrottleEntry> {
        let mut guard = self.entries.lock().await;
        if let Some(entry) = guard.get(key) {
            return Arc::clone(entry);
        }
        let entry = Arc::new(ScriptThrottleEntry::new());
        guard.insert(key.to_string(), Arc::clone(&entry));
        entry
    }
}

static REGISTRY: std::sync::OnceLock<ScriptThrottleRegistry> = std::sync::OnceLock::new();

fn registry() -> &'static ScriptThrottleRegistry {
    REGISTRY.get_or_init(ScriptThrottleRegistry::new)
}

pub(crate) async fn run_with_throttle<T, F, Fut>(
    key: &str,
    config: ScriptThrottleConfig,
    f: F,
) -> Result<T, AppError>
where
    F: FnOnce() -> Fut,
    Fut: Future<Output = Result<T, AppError>>,
{
    if !config.is_enabled() {
        return f().await;
    }

    let entry = registry().entry(key).await;
    let permit =
        Arc::clone(&entry.gate)
            .acquire_owned()
            .await
            .map_err(|_| AppError::FetchRuntime {
                reason: format!("script throttle gate closed for {key}"),
            })?;

    if entry.has_previous.load(Ordering::Relaxed) {
        throttle_script_execution(config.base_ms()).await;
    }

    let result = f().await;
    entry.has_previous.store(true, Ordering::Relaxed);
    drop(permit);
    result
}

async fn throttle_script_execution(base_ms: u64) {
    let delay_ms = random_script_delay_ms(base_ms);
    if delay_ms == 0 {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tokio::sync::Barrier;
    use tokio::time::timeout;

    static TEST_KEY_COUNTER: AtomicUsize = AtomicUsize::new(0);

    fn unique_test_key(prefix: &str) -> String {
        let index = TEST_KEY_COUNTER.fetch_add(1, Ordering::SeqCst);
        format!("{prefix}-{index}")
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
    async fn throttle_serializes_same_script() {
        let task_count = 8;
        let barrier = Arc::new(Barrier::new(task_count + 1));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let config = ScriptThrottleConfig::enabled(0);
        let key = unique_test_key("throttle-serializes-same-script");
        let mut join_set = tokio::task::JoinSet::new();

        for index in 0..task_count {
            let barrier = Arc::clone(&barrier);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            let key = key.clone();
            join_set.spawn(async move {
                barrier.wait().await;
                run_with_throttle(&key, config, || async move {
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

                    sleep(Duration::from_millis(20 + index as u64)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, AppError>(())
                })
                .await
            });
        }

        barrier.wait().await;
        while let Some(result) = join_set.join_next().await {
            result.expect("join").expect("throttle");
        }

        assert_eq!(peak.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn throttle_allows_different_scripts() {
        let barrier = Arc::new(Barrier::new(3));
        let overlap = Arc::new(Barrier::new(2));
        let active = Arc::new(AtomicUsize::new(0));
        let peak = Arc::new(AtomicUsize::new(0));
        let config = ScriptThrottleConfig::enabled(0);
        let mut join_set = tokio::task::JoinSet::new();
        let key_a = unique_test_key("throttle-allows-different-a");
        let key_b = unique_test_key("throttle-allows-different-b");

        for key in [key_a, key_b] {
            let barrier = Arc::clone(&barrier);
            let overlap = Arc::clone(&overlap);
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            join_set.spawn(async move {
                barrier.wait().await;
                run_with_throttle(&key, config, || async move {
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

                    if timeout(Duration::from_secs(1), overlap.wait())
                        .await
                        .is_err()
                    {
                        active.fetch_sub(1, Ordering::SeqCst);
                        return Err(AppError::FetchRuntime {
                            reason: "different-script throttle test did not overlap".to_string(),
                        });
                    }

                    sleep(Duration::from_millis(40)).await;
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok::<_, AppError>(())
                })
                .await
            });
        }

        barrier.wait().await;
        while let Some(result) = join_set.join_next().await {
            result.expect("join").expect("throttle");
        }

        assert!(peak.load(Ordering::SeqCst) >= 2);
    }
}

//! Waits for a slow reply while checking that the server is still there,
//! so a long wait can be told apart from a broken one. Contract: reports
//! only; the time limit on the work itself decides when to give up.

use std::future::Future;
use std::pin::pin;
use std::time::Duration;

use crate::progress::{Progress, Stage};

/// How long to wait between connection checks.
pub(crate) const CHECK_EVERY: Duration = Duration::from_secs(10);

/// Finish `work`. Each time `every` passes with no result, run `check` and
/// report whether the server answered it.
pub(crate) async fn watch<T, F>(
    work: impl Future<Output = T>,
    every: Duration,
    check: impl Fn() -> F,
    progress: &dyn Progress,
) -> T
where
    F: Future<Output = bool>,
{
    let mut work = pin!(work);
    loop {
        match tokio::time::timeout(every, work.as_mut()).await {
            Ok(result) => return result,
            Err(_) => progress.stage(Stage::Connection {
                answering: check().await,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::time::Duration;

    use super::watch;
    use crate::progress::{Progress, Stage};

    #[derive(Default)]
    struct Stages(Mutex<Vec<Stage>>);

    impl Progress for Stages {
        fn stage(&self, stage: Stage) {
            self.0.lock().expect("not poisoned").push(stage);
        }
    }

    fn run<T>(future: impl Future<Output = T>) -> T {
        tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(future)
    }

    #[test]
    fn given_slow_work_then_checks_are_reported_and_the_result_returned() {
        for answering in [true, false] {
            let stages = Stages::default();
            let checked = AtomicBool::new(false);
            // Finishes only after a check, however slow the machine is.
            let work = async {
                while !checked.load(Ordering::Relaxed) {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                7
            };

            let result = run(watch(
                work,
                Duration::from_millis(10),
                || async {
                    checked.store(true, Ordering::Relaxed);
                    answering
                },
                &stages,
            ));

            assert_eq!(result, 7);
            let stages = stages.0.lock().expect("not poisoned");
            assert!(!stages.is_empty());
            assert!(
                stages
                    .iter()
                    .all(|stage| *stage == Stage::Connection { answering })
            );
        }
    }

    #[test]
    fn given_quick_work_then_nothing_is_checked() {
        let stages = Stages::default();

        let result = run(watch(
            async { 1 },
            Duration::from_secs(60),
            || async { panic!("no check is needed") },
            &stages,
        ));

        assert_eq!(result, 1);
        assert!(stages.0.lock().expect("not poisoned").is_empty());
    }
}

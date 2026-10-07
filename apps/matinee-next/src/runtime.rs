//! The one Tokio runtime owned by the Matinee application.
//!
//! Jellyfin HTTP and other service work run here. The GPUI thread awaits a
//! oneshot and never polls those futures itself. Dropping the runtime aborts
//! leftover tasks after a short timeout, which drops in-flight HTTP futures.
//!
//! # Final work
//!
//! Work that must outlive the screen that started it, such as the Player's
//! final Jellyfin stop report, goes through [`ServiceRuntime::spawn_final`].
//! Leaving a screen does not wait for it: the runtime stays alive. An orderly
//! application exit calls [`ServiceRuntime::drain_final`] before the runtime
//! is destroyed. That is the only place this module blocks a caller, and it
//! is bounded by [`FINAL_WORK_BOUND`] from the first drain, however many
//! times drain is called. Dropping the runtime drains too, so it is never
//! torn down while final work still has time left.

use std::future::Future;
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

const SHUTDOWN: Duration = Duration::from_secs(2);

/// How long an orderly exit waits for final work, counted from the first drain.
pub const FINAL_WORK_BOUND: Duration = Duration::from_secs(2);

/// How a drain ended. Neither outcome stops the exit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Drain {
    Settled,
    TimedOut,
}

pub struct ServiceRuntime {
    runtime: Option<Runtime>,
    final_work: Arc<FinalWork>,
    exit_deadline: OnceLock<Instant>,
}

/// Count of final tasks that have not finished, with a wake-up for drain.
#[derive(Default)]
struct FinalWork {
    pending: Mutex<usize>,
    settled: Condvar,
}

/// Held by one final task. Dropping it, on completion or cancellation, settles it.
struct FinalGuard(Arc<FinalWork>);

impl Drop for FinalGuard {
    fn drop(&mut self) {
        let mut pending = self
            .0
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        *pending = pending.saturating_sub(1);
        self.0.settled.notify_all();
    }
}

impl ServiceRuntime {
    pub fn new() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("matinee-service")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Some(runtime),
            final_work: Arc::default(),
            exit_deadline: OnceLock::new(),
        })
    }

    /// Start `future` on the service runtime.
    ///
    /// The join handle aborts the task when the caller drops the window's
    /// in-flight slot. The receiver is what GPUI awaits.
    pub fn spawn<T>(
        &self,
        future: impl Future<Output = T> + Send + 'static,
    ) -> (JoinHandle<()>, tokio::sync::oneshot::Receiver<T>)
    where
        T: Send + 'static,
    {
        let (tx, rx) = tokio::sync::oneshot::channel();
        let handle = self.runtime().spawn(async move {
            let value = future.await;
            let _ = tx.send(value);
        });
        (handle, rx)
    }

    /// Start work that an orderly exit should give a bounded chance to finish.
    ///
    /// Nothing waits for it unless the application is exiting.
    pub fn spawn_final(&self, future: impl Future<Output = ()> + Send + 'static) {
        *self
            .final_work
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) += 1;
        let guard = FinalGuard(Arc::clone(&self.final_work));
        self.runtime().spawn(async move {
            let _guard = guard;
            future.await;
        });
    }

    /// Block the calling thread until final work finishes or the exit bound
    /// passes. Call this only during an orderly application exit.
    pub fn drain_final(&self) -> Drain {
        self.drain_final_within(FINAL_WORK_BOUND)
    }

    /// [`Self::drain_final`] with another bound. Tests use a short one.
    pub(crate) fn drain_final_within(&self, bound: Duration) -> Drain {
        let deadline = *self.exit_deadline.get_or_init(|| Instant::now() + bound);
        let mut pending = self
            .final_work
            .pending
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        while *pending > 0 {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Drain::TimedOut;
            }
            pending = self
                .final_work
                .settled
                .wait_timeout(pending, left)
                .unwrap_or_else(|poison| poison.into_inner())
                .0;
        }
        Drain::Settled
    }

    fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("service runtime is still running")
    }
}

impl Drop for ServiceRuntime {
    fn drop(&mut self) {
        // Already drained during an orderly exit, this returns at once.
        self.drain_final();
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[test]
    fn one_runtime_runs_a_task() {
        let runtime = ServiceRuntime::new().unwrap();
        let (task, rx) = runtime.spawn(async { 7 });
        assert_eq!(rx.blocking_recv().unwrap(), 7);
        task.abort();
    }

    #[test]
    fn shutdown_stops_in_flight_work() {
        let mut runtime = ServiceRuntime::new().unwrap();
        let (task, rx) = runtime.spawn(async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            1
        });
        if let Some(inner) = runtime.runtime.take() {
            inner.shutdown_timeout(Duration::from_millis(50));
        }
        assert!(rx.blocking_recv().is_err());
        task.abort();
    }
    fn flag() -> (Arc<AtomicBool>, Arc<AtomicBool>) {
        let flag = Arc::new(AtomicBool::new(false));
        (Arc::clone(&flag), flag)
    }

    #[test]
    fn final_work_does_not_hold_up_leaving_a_screen() {
        let runtime = ServiceRuntime::new().unwrap();
        let (done, seen) = flag();
        runtime.spawn_final(async move {
            tokio::time::sleep(Duration::from_millis(200)).await;
            done.store(true, Ordering::SeqCst);
        });
        // The runtime keeps serving the shell while the report is in flight.
        let (_task, rx) = runtime.spawn(async { 7 });
        assert_eq!(rx.blocking_recv().unwrap(), 7);
        assert!(!seen.load(Ordering::SeqCst));
        assert_eq!(
            runtime.drain_final_within(Duration::from_secs(2)),
            Drain::Settled
        );
        assert!(seen.load(Ordering::SeqCst));
    }

    #[test]
    fn drain_waits_for_final_work_before_teardown() {
        let runtime = ServiceRuntime::new().unwrap();
        let (done, seen) = flag();
        runtime.spawn_final(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            done.store(true, Ordering::SeqCst);
        });
        // No explicit drain: dropping the runtime drains first.
        drop(runtime);
        assert!(seen.load(Ordering::SeqCst));
    }

    #[test]
    fn hanging_final_work_is_abandoned_at_the_bound() {
        let runtime = ServiceRuntime::new().unwrap();
        runtime.spawn_final(std::future::pending());
        let started = Instant::now();
        let bound = Duration::from_millis(150);
        assert_eq!(runtime.drain_final_within(bound), Drain::TimedOut);
        let waited = started.elapsed();
        assert!(waited >= bound, "{waited:?}");
        assert!(waited < Duration::from_secs(1), "{waited:?}");
        // A second drain, and the drop, share the deadline instead of waiting again.
        assert_eq!(runtime.drain_final(), Drain::TimedOut);
        drop(runtime);
        assert!(started.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn an_idle_runtime_drains_at_once() {
        let runtime = ServiceRuntime::new().unwrap();
        let started = Instant::now();
        assert_eq!(runtime.drain_final(), Drain::Settled);
        assert!(started.elapsed() < Duration::from_millis(100));
    }
}

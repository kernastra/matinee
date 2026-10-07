//! The one Tokio runtime owned by the Matinee application.
//!
//! Jellyfin HTTP and other service work run here. The GPUI thread awaits a
//! oneshot and never polls those futures itself. Dropping the runtime aborts
//! leftover tasks after a short timeout, which drops in-flight HTTP futures.

use std::future::Future;
use std::time::Duration;

use tokio::runtime::Runtime;
use tokio::task::JoinHandle;

const SHUTDOWN: Duration = Duration::from_secs(2);

pub struct ServiceRuntime {
    runtime: Option<Runtime>,
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

    fn runtime(&self) -> &Runtime {
        self.runtime
            .as_ref()
            .expect("service runtime is still running")
    }
}

impl Drop for ServiceRuntime {
    fn drop(&mut self) {
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_timeout(SHUTDOWN);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}

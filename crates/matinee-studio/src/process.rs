//! Direct process execution. There is no shell.
//!
//! Codex is `program` plus an argument vector. `sh -c`, `cmd /C`, and
//! PowerShell are not used. The caller polls [`TokioProcess`] on a Tokio
//! runtime this crate does not create. The child stays owned here.
//! `kill_on_drop` kills it when this future is dropped, and a short-lived
//! reap thread waits so the process does not remain a zombie. That thread is
//! not a process manager. A timeout kills and reaps the child on both the
//! file-backed path and the piped path (`codex login status`), then returns
//! [`ProcessError::Timeout`].

use std::fs::File;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;

use crate::error::StudioError;

pub struct ProcessSpec {
    pub program: std::path::PathBuf,
    pub args: Vec<String>,
    pub current_dir: Option<std::path::PathBuf>,
    pub timeout: Duration,
    pub stdout_path: Option<std::path::PathBuf>,
    pub stderr_path: Option<std::path::PathBuf>,
    pub capture_limit: usize,
}

#[derive(Clone, Debug)]
pub struct ProcessOutput {
    pub success: bool,
    pub code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug)]
pub enum ProcessError {
    Timeout,
    Failed { message: String },
}

impl ProcessError {
    pub(crate) fn into_studio(self, timeout: StudioError) -> StudioError {
        match self {
            Self::Timeout => timeout,
            Self::Failed { message } => StudioError::filesystem(message),
        }
    }
}

pub trait ProcessRunner: Send + Sync {
    fn run(
        &self,
        spec: ProcessSpec,
    ) -> impl std::future::Future<Output = Result<ProcessOutput, ProcessError>> + Send;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TokioProcess;

impl ProcessRunner for TokioProcess {
    async fn run(&self, spec: ProcessSpec) -> Result<ProcessOutput, ProcessError> {
        let mut command = tokio::process::Command::new(&spec.program);
        command.args(&spec.args).kill_on_drop(true);
        if let Some(directory) = &spec.current_dir {
            command.current_dir(directory);
        }
        if let (Some(stdout_path), Some(stderr_path)) = (&spec.stdout_path, &spec.stderr_path) {
            let stdout = File::create(stdout_path).map_err(failed)?;
            let stderr = File::create(stderr_path).map_err(failed)?;
            command
                .stdout(Stdio::from(stdout))
                .stderr(Stdio::from(stderr));
            let mut child = ReapOnDrop::new(command.spawn().map_err(failed)?);
            let status = match tokio::time::timeout(spec.timeout, child.get().wait()).await {
                Ok(result) => result.map_err(failed)?,
                Err(_) => {
                    let _ = child.get().kill().await;
                    let _ = child.get().wait().await;
                    child.disarm();
                    return Err(ProcessError::Timeout);
                }
            };
            child.disarm();
            return Ok(ProcessOutput {
                success: status.success(),
                code: status.code(),
                stdout: excerpt(stdout_path, spec.capture_limit),
                stderr: excerpt(stderr_path, spec.capture_limit),
            });
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let mut child = ReapOnDrop::new(command.spawn().map_err(failed)?);
        let stdout = child.get().stdout.take();
        let stderr = child.get().stderr.take();
        let limit = spec.capture_limit;
        let stdout_task = tokio::spawn(async move {
            match stdout {
                Some(pipe) => read_pipe(pipe, limit).await,
                None => String::new(),
            }
        });
        let stderr_task = tokio::spawn(async move {
            match stderr {
                Some(pipe) => read_pipe(pipe, limit).await,
                None => String::new(),
            }
        });
        let status = match tokio::time::timeout(spec.timeout, child.get().wait()).await {
            Ok(result) => result.map_err(failed)?,
            Err(_) => {
                let _ = child.get().kill().await;
                let _ = child.get().wait().await;
                child.disarm();
                stdout_task.abort();
                stderr_task.abort();
                return Err(ProcessError::Timeout);
            }
        };
        child.disarm();
        let stdout = stdout_task.await.unwrap_or_default();
        let stderr = stderr_task.await.unwrap_or_default();
        Ok(ProcessOutput {
            success: status.success(),
            code: status.code(),
            stdout,
            stderr,
        })
    }
}

struct ReapOnDrop {
    child: Option<tokio::process::Child>,
}

impl ReapOnDrop {
    fn new(child: tokio::process::Child) -> Self {
        Self { child: Some(child) }
    }

    fn get(&mut self) -> &mut tokio::process::Child {
        self.child.as_mut().expect("child")
    }

    fn disarm(&mut self) {
        self.child = None;
    }
}

impl Drop for ReapOnDrop {
    fn drop(&mut self) {
        let Some(mut child) = self.child.take() else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("matinee-reap".into())
            .spawn(move || {
                let _ = child.start_kill();
                for _ in 0..100 {
                    match child.try_wait() {
                        Ok(Some(_)) | Err(_) => return,
                        Ok(None) => std::thread::sleep(Duration::from_millis(10)),
                    }
                }
            });
    }
}

async fn read_pipe<R>(mut pipe: R, limit: usize) -> String
where
    R: tokio::io::AsyncRead + Unpin,
{
    let mut bytes = Vec::new();
    let _ = tokio::io::AsyncReadExt::read_to_end(&mut pipe, &mut bytes).await;
    bounded_text(&bytes, limit)
}

fn failed(error: std::io::Error) -> ProcessError {
    ProcessError::Failed {
        message: error.to_string(),
    }
}

fn excerpt(path: &Path, limit: usize) -> String {
    std::fs::read_to_string(path)
        .map(|contents| tail_chars(&contents, limit))
        .unwrap_or_default()
}

fn bounded_text(bytes: &[u8], limit: usize) -> String {
    tail_chars(&String::from_utf8_lossy(bytes), limit)
}

fn tail_chars(contents: &str, limit: usize) -> String {
    let characters = contents.trim().chars().collect::<Vec<_>>();
    characters[characters.len().saturating_sub(limit)..]
        .iter()
        .collect()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn sleep_spec(seconds: &str, timeout: Duration) -> ProcessSpec {
        ProcessSpec {
            program: Path::new("/bin/sleep").into(),
            args: vec![seconds.to_string()],
            current_dir: None,
            timeout,
            stdout_path: None,
            stderr_path: None,
            capture_limit: 64,
        }
    }

    fn marker(label: &str) -> String {
        format!(
            "{}",
            40_000 + (std::process::id() % 1_000) + if label == "drop" { 10_000 } else { 0 }
        )
    }

    fn matching(seconds: &str) -> bool {
        let output = std::process::Command::new("pgrep")
            .args(["-f", &format!("[s]leep {seconds}")])
            .output()
            .expect("pgrep");
        output.status.success() && !output.stdout.is_empty()
    }

    async fn wait_until_gone(seconds: &str) -> bool {
        for _ in 0..40 {
            if !matching(seconds) {
                return true;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        false
    }

    #[tokio::test]
    async fn piped_output_returns_a_short_command() {
        let output = TokioProcess
            .run(ProcessSpec {
                program: Path::new("/bin/echo").into(),
                args: vec!["matinee-ok".into()],
                current_dir: None,
                timeout: Duration::from_secs(5),
                stdout_path: None,
                stderr_path: None,
                capture_limit: 64,
            })
            .await
            .expect("echo");
        assert!(output.success);
        assert!(output.stdout.contains("matinee-ok"));
    }

    #[tokio::test]
    async fn piped_timeout_kills_the_child() {
        let seconds = marker("timeout");
        let run = TokioProcess.run(sleep_spec(&seconds, Duration::from_millis(200)));
        tokio::pin!(run);
        let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
        loop {
            if matching(&seconds) {
                break;
            }
            tokio::select! {
                result = &mut run => panic!("sleep ended before it was visible: {result:?}"),
                _ = tokio::time::sleep(Duration::from_millis(20)) => {
                    assert!(tokio::time::Instant::now() < deadline, "sleep never started");
                }
            }
        }
        let result = tokio::time::timeout(Duration::from_secs(2), run)
            .await
            .expect("timeout did not return");
        assert!(matches!(result, Err(ProcessError::Timeout)));
        assert!(wait_until_gone(&seconds).await, "sleep survived timeout");
    }

    #[tokio::test]
    async fn dropping_a_piped_run_kills_the_child() {
        let seconds = marker("drop");
        {
            let run = TokioProcess.run(sleep_spec(&seconds, Duration::from_secs(30)));
            tokio::pin!(run);
            let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
            loop {
                if matching(&seconds) {
                    break;
                }
                tokio::select! {
                    result = &mut run => panic!("sleep ended before it was visible: {result:?}"),
                    _ = tokio::time::sleep(Duration::from_millis(20)) => {
                        assert!(tokio::time::Instant::now() < deadline, "sleep never started");
                    }
                }
            }
        }
        assert!(
            wait_until_gone(&seconds).await,
            "sleep survived cancellation"
        );
    }
}

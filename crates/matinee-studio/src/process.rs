//! Direct process execution. There is no shell.
//!
//! Codex is `program` plus an argument vector. `sh -c`, `cmd /C`, and
//! PowerShell are not used. The caller polls [`TokioProcess`] on a Tokio
//! runtime this crate does not create. `kill_on_drop` stops the child when
//! the future is dropped. A timeout kills a file-backed child and returns
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
            let mut child = command.spawn().map_err(failed)?;
            let status = match tokio::time::timeout(spec.timeout, child.wait()).await {
                Ok(result) => result.map_err(failed)?,
                Err(_) => {
                    let _ = child.kill().await;
                    let _ = child.wait().await;
                    return Err(ProcessError::Timeout);
                }
            };
            return Ok(ProcessOutput {
                success: status.success(),
                code: status.code(),
                stdout: excerpt(stdout_path, spec.capture_limit),
                stderr: excerpt(stderr_path, spec.capture_limit),
            });
        }

        command.stdout(Stdio::piped()).stderr(Stdio::piped());
        let child = command.spawn().map_err(failed)?;
        match tokio::time::timeout(spec.timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => Ok(ProcessOutput {
                success: output.status.success(),
                code: output.status.code(),
                stdout: bounded_text(&output.stdout, spec.capture_limit),
                stderr: bounded_text(&output.stderr, spec.capture_limit),
            }),
            Ok(Err(error)) => Err(failed(error)),
            Err(_) => Err(ProcessError::Timeout),
        }
    }
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

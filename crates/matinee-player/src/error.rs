//! Failures a caller can handle. Opening a player never panics because a
//! library is missing.

#![forbid(unsafe_code)]

use std::fmt;

/// Typed playback failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PlayerError {
    /// No usable libmpv was found. The process can keep running.
    LibraryMissing { searched: Vec<String> },
    /// A library loaded, but the client API was missing or not version 2.
    LibraryIncompatible { path: String, detail: String },
    /// libmpv was found and then failed during startup.
    Initialization(String),
    /// The command does not apply to the current snapshot.
    InvalidCommand(String),
    /// The owner thread has already exited.
    EngineStopped,
    /// libmpv reported a failure while applying a command.
    Engine(String),
}

impl fmt::Display for PlayerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LibraryMissing { searched } => {
                write!(
                    f,
                    "libmpv was not found (looked for {})",
                    searched.join(", ")
                )
            }
            Self::LibraryIncompatible { path, detail } => {
                write!(f, "libmpv at {path} is incompatible: {detail}")
            }
            Self::Initialization(detail) => write!(f, "playback engine failed to start: {detail}"),
            Self::InvalidCommand(detail) => write!(f, "invalid playback command: {detail}"),
            Self::EngineStopped => write!(f, "playback engine has stopped"),
            Self::Engine(detail) => write!(f, "playback engine: {detail}"),
        }
    }
}

impl std::error::Error for PlayerError {}

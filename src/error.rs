/// Error types for the mneme event sourcing system.
///
/// This module provides a comprehensive error handling system for the mneme library,
/// including detailed error context and correlation IDs for debugging and tracing.
use eventstore::ClientSettingsParseError;
use std::fmt::Debug;
use thiserror::Error;

use crate::{EventStreamId, EventStreamVersion};

/// Represents errors that can occur in the mneme event sourcing system
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates a failure to parse event store connection settings
    #[error(transparent)]
    EventStoreSettings(#[from] ClientSettingsParseError),

    /// Indicates a failure to deserialize an event from the store
    #[error(transparent)]
    EventDeserializationError(#[from] serde_json::error::Error),

    /// Indicates that a requested event stream was not found
    #[error("Stream not found: {stream_id}", stream_id = .0.to_string())]
    EventStoreStreamNotFound(EventStreamId),

    /// Indicates a version mismatch when appending to a stream
    #[error("Version mismatch for stream '{stream:?}': {:?}", match (&expected, &actual) {
        (Some(e), Some(a)) => format!("expected version {:?}, but stream is at version {:?}", e, a),
        (Some(e), None) => format!("expected version {:?}, but stream does not exist", e),
        (None, Some(a)) => format!("stream exists at version {:?}, but no version was expected", a),
        (None, None) => "invalid version state".to_string()
    })]
    EventStoreVersionMismatch {
        stream: EventStreamId,
        expected: Option<EventStreamVersion>,
        actual: Option<EventStreamVersion>,
        #[source]
        source: eventstore::Error,
    },

    /// Indicates a general event store error
    #[error(transparent)]
    EventStoreOther(#[from] eventstore::Error),

    /// Indicates a failure in command execution
    #[error("Command failed (attempt {attempt} of {max_attempts}): {message}")]
    CommandFailed {
        message: String,
        attempt: u32,
        max_attempts: u32,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    /// Indicates that maximum retry attempts were exceeded
    #[error("Command execution exceeded maximum retries ({max_retries}) for stream '{stream}'")]
    MaxRetriesExceeded { stream: String, max_retries: u32 },

    /// Indicates an invalid configuration parameter
    #[error("Invalid configuration{}: {message}", parameter.as_ref().map(|p| format!(" parameter '{p}'")).unwrap_or_default())]
    InvalidConfig {
        message: String,
        parameter: Option<String>,
    },
}

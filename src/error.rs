use eventstore::ClientSettingsParseError;
use std::fmt::Debug;
use thiserror::Error;

use crate::event_store::{EventStreamId, EventStreamVersion};

#[derive(Debug, Error)]
pub enum Error {
    #[error(transparent)]
    EventStoreSettings(#[from] ClientSettingsParseError),

    #[error(transparent)]
    EventDeserializationError(#[from] serde_json::error::Error),

    #[error("Stream not found: {stream_id}", stream_id = .0.to_string())]
    EventStoreStreamNotFound(EventStreamId),

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

    #[error(transparent)]
    EventStoreOther(#[from] eventstore::Error),

    #[error("Command failed (attempt {attempt} of {max_attempts}): {message}")]
    CommandFailed {
        message: String,
        attempt: u32,
        max_attempts: u32,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },

    #[error("Command execution exceeded maximum retries ({max_retries}) for stream '{stream}'")]
    MaxRetriesExceeded { stream: String, max_retries: u32 },

    #[error("Invalid configuration{}: {message}", parameter.as_ref().map(|p| format!(" parameter '{p}'")).unwrap_or_default())]
    InvalidConfig {
        message: String,
        parameter: Option<String>,
    },
}

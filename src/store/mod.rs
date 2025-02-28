//! Event store implementation for persisting and retrieving events.
//!
//! This module provides the core event store functionality for the event sourcing system.
//! It handles:
//! - Event persistence
//! - Stream management
//! - Event retrieval
//! - Optimistic concurrency control
//!
//! # Examples
//!
//! Basic event store operations:
//!
//! ```rust,no_run
//! use mneme::{EventStore, ConnectionSettings, Event, EventStreamId};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! enum OrderEvent {
//!     Created { id: String }
//! }
//!
//! impl Event for OrderEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             OrderEvent::Created { .. } => "OrderCreated".to_string()
//!         }
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize from environment
//! let settings = ConnectionSettings::from_env()?;
//! let store = EventStore::new(&settings)?;
//!
//! // Read from a stream
//! let stream_id = EventStreamId::new();
//! let mut stream = store
//!     .stream_builder(stream_id.clone())
//!     .max_count(100)
//!     .read::<OrderEvent>()
//!     .await?;
//!
//! while let Some((event, version)) = stream.next().await? {
//!     println!("Event at version {}: {:?}", version.value(), event);
//! }
//!
//! // Write to a stream
//! let events = vec![OrderEvent::Created {
//!     id: "order-123".to_string()
//! }];
//!
//! store.stream_writer(stream_id)
//!     .append(events)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! Using optimistic concurrency:
//!
//! ```rust,no_run
//! # use mneme::{EventStore, ConnectionSettings, Event, EventStreamId};
//! # use serde::{Serialize, Deserialize};
//! # #[derive(Debug, Serialize, Deserialize)]
//! # enum OrderEvent {
//! #     Created { id: String }
//! # }
//! # impl Event for OrderEvent {
//! #     fn event_type(&self) -> String {
//! #         "OrderCreated".to_string()
//! #     }
//! # }
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! # let settings = ConnectionSettings::from_env()?;
//! # let store = EventStore::new(&settings)?;
//! let stream_id = EventStreamId::new();
//! let events = vec![OrderEvent::Created {
//!     id: "order-123".to_string()
//! }];
//!
//! // Write only if stream is at version 5
//! store.stream_writer(stream_id)
//!     .expected_version(5)
//!     .append(events)
//!     .await?;
//! # Ok(())
//! # }
//! ```

mod config;
mod delay;
mod impl_store;
mod settings;
mod stream;

pub use config::ExecuteConfig;
pub use impl_store::{EventStore, EventStoreOps};
pub use settings::ConnectionSettings;
pub use stream::{EventStream, EventStreamId, EventStreamVersion};

use crate::command::Command;
use crate::error::Error;
use crate::event::Event;

/// Execute a command with metrics collection
pub async fn execute<E, C, S>(
    command: C,
    event_store: &mut S,
    config: ExecuteConfig,
) -> Result<(), Error>
where
    E: Event,
    C: Command<E> + Clone + Send,
    S: EventStoreOps + Send,
{
    // Create metrics for this execution
    let mut retries = 0;
    let mut command = command;

    let result = loop {
        if retries > config.max_retries() {
            break Err(Error::MaxRetriesExceeded {
                stream: command.event_stream_id().to_string(),
                max_retries: config.max_retries(),
            });
        }

        let mut expected_version = None;

        // Read and apply existing events from the stream to rebuild the aggregate state
        let read_result = event_store.read_stream(command.event_stream_id()).await;

        match read_result {
            // Stream doesn't exist yet, which is fine for a new aggregate
            Err(Error::EventStoreOther(eventstore::Error::ResourceNotFound)) => {}

            // Other errors should be propagated
            Err(other) => {
                break Err(other);
            }

            // Stream exists, so apply all events to rebuild the aggregate state
            Ok(mut event_stream) => {
                while let Some((event, version)) = event_stream.next().await? {
                    command = command.apply(event);
                    expected_version = Some(version);
                }
            }
        }

        // Now handle the command and produce new events
        let domain_events = match command.handle() {
            Ok(events) => events,
            Err(e) => {
                break Err(Error::CommandFailed {
                    message: e.to_string(),
                    attempt: retries + 1,
                    max_attempts: config.max_retries(),
                    source: Box::new(e),
                });
            }
        };

        // Only publish if there are events to publish
        if !domain_events.is_empty() {
            // Let the command override the expected version if it wants to
            let append_options = match (command.override_expected_version(), expected_version) {
                (Some(v), _) => eventstore::AppendToStreamOptions::default()
                    .expected_revision(eventstore::ExpectedRevision::Exact(v)),
                (None, Some(v)) => eventstore::AppendToStreamOptions::default()
                    .expected_revision(eventstore::ExpectedRevision::Exact(v.value())),
                (None, None) => Default::default(),
            };

            match event_store
                .publish(command.event_stream_id(), domain_events, &append_options)
                .await
            {
                Ok(_) => {
                    break Ok(());
                }
                Err(Error::EventStoreVersionMismatch { .. }) => {
                    // Calculate delay with exponential backoff and jitter
                    let delay = config.retry_delay().calculate_delay(retries);
                    tokio::time::sleep(delay).await;

                    // Mark command as being retried and increment retry counter
                    command = command.mark_retry();
                    retries += 1;
                    continue;
                }
                Err(e) => {
                    break Err(e);
                }
            }
        }

        break Ok(());
    };

    // Stop the timer before returning

    result
}

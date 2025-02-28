mod command;
mod config;
mod delay;
mod error;
mod event;
mod event_store;
mod kurrent_adapter;

pub use command::{AggregateState, Command};
pub use config::ExecuteConfig;
pub use error::Error;
pub use event::Event;
pub use event_store::{EventStore, EventStreamId, EventStreamVersion};
pub use kurrent_adapter::{ConnectionSettings, EventStream, Kurrent};

use delay::RetryDelay;

pub async fn execute<E, C, S>(
    command: C,
    event_store: &mut S,
    config: ExecuteConfig,
) -> Result<(), Error>
where
    E: Event,
    C: Command<E> + Clone + Send,
    S: EventStore + Send,
{
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

        let read_result = event_store.read_stream(command.event_stream_id()).await;

        match read_result {
            Err(Error::EventStoreOther(eventstore::Error::ResourceNotFound)) => {}

            Err(other) => {
                break Err(other);
            }

            Ok(mut event_stream) => {
                while let Some((event, version)) = event_stream.next().await? {
                    command = command.apply(event);
                    expected_version = Some(version);
                }
            }
        }

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

        if !domain_events.is_empty() {
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
                    let delay = config.retry_delay().calculate_delay(retries);
                    tokio::time::sleep(delay).await;

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

    result
}

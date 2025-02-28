mod settings;
mod stream;

pub use settings::ConnectionSettings;
pub use stream::{EventStream, EventStreamVersion};

use crate::error::Error;
use crate::event::Event;
use crate::event_store::{EventStore, EventStreamId};

use eventstore::AppendToStreamOptions;

#[derive(Clone)]
pub struct Kurrent {
    pub client: eventstore::Client,
}

impl Kurrent {
    pub fn new(settings: &ConnectionSettings) -> Result<Self, Error> {
        let client = eventstore::Client::new(settings.to_client_settings()?)?;
        Ok(Self { client })
    }

    pub fn from_env() -> Result<Self, Error> {
        let settings = ConnectionSettings::from_env()?;
        Self::new(&settings)
    }

    pub fn stream_builder(&self, stream_id: EventStreamId) -> EventStreamBuilder {
        EventStreamBuilder::new(self.clone(), stream_id)
    }

    pub fn stream_writer(&self, stream_id: EventStreamId) -> EventStreamWriter {
        EventStreamWriter::new(self.clone(), stream_id)
    }
}

impl EventStore for Kurrent {
    async fn append_to_stream(
        &mut self,
        stream_id: EventStreamId,
        options: &AppendToStreamOptions,
        events: Vec<eventstore::EventData>,
    ) -> Result<eventstore::WriteResult, Error> {
        self.client
            .append_to_stream(stream_id.clone(), options, events)
            .await
            .map_err(|source| match source {
                eventstore::Error::ResourceNotFound => Error::EventStoreStreamNotFound(stream_id),
                eventstore::Error::WrongExpectedVersion { current, expected } => {
                    Error::EventStoreVersionMismatch {
                        stream: stream_id,
                        expected: extract_revision(&expected),
                        actual: extract_current_revision(&current),
                        source,
                    }
                }
                e => Error::EventStoreOther(e),
            })
    }

    async fn publish<E: Event>(
        &mut self,
        stream_id: EventStreamId,
        events: Vec<E>,
        options: &AppendToStreamOptions,
    ) -> Result<(), Error> {
        let events: Vec<eventstore::EventData> = events
            .iter()
            .map(|event| {
                let event_type = event.event_type();
                eventstore::EventData::json(&event_type, &event)
                    .map_err(Error::EventDeserializationError)
            })
            .collect::<Result<_, _>>()?;

        self.append_to_stream(stream_id, options, events).await?;
        Ok(())
    }

    async fn read_stream<E: Event>(
        &self,
        stream_id: EventStreamId,
    ) -> Result<EventStream<E>, Error> {
        let stream = self
            .client
            .read_stream(stream_id.clone(), &Default::default())
            .await
            .map(|stream| EventStream {
                stream,
                type_marker: std::marker::PhantomData,
            })
            .map_err(|source| match source {
                eventstore::Error::ResourceNotFound => Error::EventStoreStreamNotFound(stream_id),
                e => Error::EventStoreOther(e),
            })?;
        Ok(stream)
    }
}

pub struct EventStreamBuilder {
    store: Kurrent,
    stream_id: EventStreamId,
    read_options: eventstore::ReadStreamOptions,
}

impl EventStreamBuilder {
    pub fn new(store: Kurrent, stream_id: EventStreamId) -> Self {
        Self {
            store,
            stream_id,
            read_options: Default::default(),
        }
    }

    pub fn max_count(mut self, count: u64) -> Self {
        self.read_options = self.read_options.max_count(count.try_into().unwrap());
        self
    }

    pub fn position(mut self, position: eventstore::StreamPosition<u64>) -> Self {
        self.read_options = self.read_options.position(position);
        self
    }

    pub async fn read<E: Event>(self) -> Result<EventStream<E>, Error> {
        let stream = self
            .store
            .client
            .read_stream(self.stream_id.clone(), &self.read_options)
            .await
            .map(|stream| EventStream {
                stream,
                type_marker: std::marker::PhantomData,
            })
            .map_err(|source| match source {
                eventstore::Error::ResourceNotFound => {
                    Error::EventStoreStreamNotFound(self.stream_id)
                }
                e => Error::EventStoreOther(e),
            })?;
        Ok(stream)
    }
}

pub struct EventStreamWriter {
    store: Kurrent,
    stream_id: EventStreamId,
    write_options: AppendToStreamOptions,
}

impl EventStreamWriter {
    pub fn new(store: Kurrent, stream_id: EventStreamId) -> Self {
        Self {
            store,
            stream_id,
            write_options: Default::default(),
        }
    }

    pub fn expected_version(mut self, version: u64) -> Self {
        self.write_options = self
            .write_options
            .expected_revision(eventstore::ExpectedRevision::Exact(version));
        self
    }

    pub fn any_version(mut self) -> Self {
        self.write_options = self
            .write_options
            .expected_revision(eventstore::ExpectedRevision::Any);
        self
    }

    pub fn no_stream(mut self) -> Self {
        self.write_options = self
            .write_options
            .expected_revision(eventstore::ExpectedRevision::NoStream);
        self
    }

    pub async fn append<E: Event>(self, events: Vec<E>) -> Result<eventstore::WriteResult, Error> {
        let events: Vec<eventstore::EventData> = events
            .iter()
            .map(|event| {
                let event_type = event.event_type();
                eventstore::EventData::json(&event_type, &event)
                    .map_err(Error::EventDeserializationError)
            })
            .collect::<Result<_, _>>()?;

        self.store
            .client
            .append_to_stream(self.stream_id.clone(), &self.write_options, events)
            .await
            .map_err(|source| match source {
                eventstore::Error::ResourceNotFound => {
                    Error::EventStoreStreamNotFound(self.stream_id)
                }
                eventstore::Error::WrongExpectedVersion { current, expected } => {
                    Error::EventStoreVersionMismatch {
                        stream: self.stream_id,
                        expected: extract_revision(&expected),
                        actual: extract_current_revision(&current),
                        source,
                    }
                }
                e => Error::EventStoreOther(e),
            })
    }
}

fn extract_revision(expected: &eventstore::ExpectedRevision) -> Option<EventStreamVersion> {
    match expected {
        eventstore::ExpectedRevision::Exact(v) => Some(EventStreamVersion::new(*v)),
        _ => None,
    }
}

fn extract_current_revision(current: &eventstore::CurrentRevision) -> Option<EventStreamVersion> {
    match current {
        eventstore::CurrentRevision::Current(v) => Some(EventStreamVersion::new(*v)),
        _ => None,
    }
}

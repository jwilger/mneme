use eventstore::AppendToStreamOptions;
use uuid::Uuid;

use crate::{Error, Event, EventStream};

pub trait EventStore {
    fn append_to_stream(
        &mut self,
        stream_id: EventStreamId,
        options: &AppendToStreamOptions,
        events: Vec<eventstore::EventData>,
    ) -> impl std::future::Future<Output = Result<eventstore::WriteResult, Error>> + Send;

    fn publish<E: Event>(
        &mut self,
        stream_id: EventStreamId,
        events: Vec<E>,
        options: &AppendToStreamOptions,
    ) -> impl std::future::Future<Output = Result<(), Error>> + Send;

    fn read_stream<E: Event>(
        &self,
        stream_id: EventStreamId,
    ) -> impl std::future::Future<Output = Result<EventStream<E>, Error>> + Send;
}

#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub struct EventStreamId(pub Uuid);

impl EventStreamId {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_uuid(uuid: Uuid) -> Self {
        Self(uuid)
    }
}

impl Default for EventStreamId {
    fn default() -> Self {
        Self(Uuid::new_v4())
    }
}

impl std::fmt::Display for EventStreamId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventStreamVersion(u64);

impl EventStreamVersion {
    pub fn new(version: u64) -> Self {
        Self(version)
    }

    pub fn value(&self) -> u64 {
        self.0
    }
}

use crate::error::Error;
use crate::event::Event;
use crate::event_store::EventStreamId;
use bytes::Bytes;
use std::marker::PhantomData;

impl eventstore::StreamName for EventStreamId {
    fn into_stream_name(self) -> Bytes {
        Bytes::from(self.0.to_string())
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

pub struct EventStream<E: Event> {
    pub(crate) stream: eventstore::ReadStream,
    pub(crate) type_marker: PhantomData<E>,
}

impl<E: Event> EventStream<E> {
    pub async fn next(&mut self) -> Result<Option<(E, EventStreamVersion)>, Error> {
        match self.stream.next().await.map_err(Error::EventStoreOther)? {
            None => Ok(None),
            Some(resolved) => {
                let original = resolved.get_original_event();
                let stream_version = EventStreamVersion::new(original.revision);
                let event = original
                    .as_json::<E>()
                    .map_err(Error::EventDeserializationError)?;
                Ok(Some((event, stream_version)))
            }
        }
    }
}

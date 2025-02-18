use crate::{EventStore, EventStreamVersion, StorageError};
use eventstore::Client;
use tokio_stream::Stream;

pub struct Kurrent {
    pub client: Client,
}

impl<E: Send> EventStore<E> for Kurrent {
    async fn publish(
        &mut self,
        _events: Vec<E>,
        _expected_version: Option<EventStreamVersion>,
    ) -> Result<(), StorageError> {
        todo!()
    }

    async fn read_stream(
        &self,
        _stream_id: crate::EventStreamId,
    ) -> Result<impl Stream<Item = E>, StorageError> {
        Ok(tokio_stream::empty())
    }
}

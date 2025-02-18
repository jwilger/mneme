use crate::{EventStore, EventStreamId, EventStreamVersion, StorageError};
use eventstore::{Client, ClientSettings};
use tokio_stream::Stream;

#[derive(Clone, thiserror::Error, Debug)]
pub enum KurrentError {
    #[error("Connection error: {0}")]
    ConnectionError(#[from] eventstore::Error),
}

pub struct Kurrent<E> {
    pub client: Client,
    type_marker: std::marker::PhantomData<E>,
}

impl<E> Kurrent<E> {
    pub fn new(settings: ClientSettings) -> Result<Self, KurrentError> {
        let client = Client::new(settings)?;
        Ok(Self {
            client,
            type_marker: std::marker::PhantomData,
        })
    }
}

impl<E: Send + Sync> EventStore<E> for Kurrent<E> {
    async fn publish(
        &mut self,
        _events: Vec<E>,
        _expected_version: Option<EventStreamVersion>,
    ) -> Result<(), StorageError> {
        todo!()
    }

    async fn read_stream(
        &self,
        _stream_id: &EventStreamId,
    ) -> Result<impl Stream<Item = E>, StorageError> {
        Ok(tokio_stream::empty())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::EventStreamId;
    use tokio_stream::StreamExt;
    use uuid::Uuid;

    struct Event;

    #[tokio::test]
    async fn read_stream_on_empty_stream() {
        let event_store = Kurrent::<Event>::new("esdb://localhost:2113".parse().unwrap())
            .expect("could not instantiate Kurrent");
        let stream_id = EventStreamId::try_new(Uuid::new_v4()).expect("invalid stream ID");
        let mut result = event_store
            .read_stream(&stream_id)
            .await
            .expect("read_stream failed");
        assert!(result.next().await.is_none());
    }
}

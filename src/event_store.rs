use eventstore::AppendToStreamOptions;

use crate::{Error, Event, EventStream, EventStreamId};

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

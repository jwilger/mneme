use crate::EventStreamVersion;
use crate::event::Event;
use crate::event_store::EventStreamId;
use std::fmt::Debug;

pub trait Command: Clone {
    type Event: Event;
    type State: AggregateState<Self::Event>;
    type Error: std::error::Error + Send + Sync + 'static;

    fn handle(&self) -> Result<Vec<Self::Event>, Self::Error>;

    fn event_stream_id(&self) -> EventStreamId;

    fn get_state(&self) -> Self::State;

    fn set_state(&mut self, state: Self::State);

    fn mark_retry(&self) -> Self
    where
        Self: Sized + Clone,
    {
        self.clone()
    }

    fn override_expected_version(&self) -> Option<EventStreamVersion> {
        None
    }

    fn apply(&mut self, event: &Self::Event)
    where
        Self: Sized,
    {
        self.set_state(self.get_state().apply(event));
    }
}

pub trait AggregateState<E: Event>: Debug + Sized {
    fn apply(&self, event: &E) -> Self;
}

impl<E: Event> AggregateState<E> for () {
    fn apply(&self, _: &E) {}
}

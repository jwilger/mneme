use crate::event::Event;
use crate::event_store::EventStreamId;
use std::fmt::Debug;

pub trait Command<E: Event> {
    type State: AggregateState<E>;
    type Error: std::error::Error + Send + Sync + 'static;

    fn handle(&self) -> Result<Vec<E>, Self::Error>;

    fn event_stream_id(&self) -> EventStreamId;

    fn get_state(&self) -> Self::State;

    fn set_state(&self, state: Self::State) -> Self;

    fn mark_retry(&self) -> Self
    where
        Self: Sized + Clone,
    {
        self.clone()
    }

    fn override_expected_version(&self) -> Option<u64> {
        None
    }

    fn apply(&mut self, event: E) -> Self
    where
        Self: Sized,
    {
        self.set_state(self.get_state().apply(event))
    }
}

impl<E: Event> Command<E> for () {
    type State = ();
    type Error = std::convert::Infallible;

    fn handle(&self) -> Result<Vec<E>, Self::Error> {
        Ok(vec![])
    }
    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId::new()
    }
    fn get_state(&self) -> Self::State {}
    fn set_state(&self, _: Self::State) -> Self {}
    fn mark_retry(&self) -> Self {}
}

pub trait AggregateState<E: Event>: Debug + Sized {
    fn apply(&self, event: E) -> Self;
}

impl<E: Event> AggregateState<E> for () {
    fn apply(&self, _: E) {}
}

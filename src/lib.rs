use nutype::nutype;
use std::error::Error;

/// Represents an error that occurs during storage operations.
#[derive(Clone, thiserror::Error, Debug)]
#[error("Storage error: {0}")]
pub struct StorageError(String);

/// Represents an identifier for an event stream.
///
/// This struct ensures that the identifier is trimmed and not empty.
#[nutype(sanitize(trim), validate(not_empty), derive(Debug, PartialEq))]
pub struct EventStreamId(String);

#[cfg(test)]
impl EventStreamId {
    /// Creates a new `EventStreamId` for testing purposes.
    ///
    /// Not available in the public API.
    ///
    /// # Arguments
    ///
    /// * `value` - A string value to be used as the event stream ID.
    ///
    /// # Panics
    ///
    /// This function will panic if the provided value is not a valid event stream ID.
    fn new_test(value: String) -> Self {
        match Self::try_new(value) {
            Ok(id) => id,
            Err(_) => panic!("Invalid event stream ID"),
        }
    }
}

/// Represents a query for event streams.
#[derive(Debug, PartialEq)]
pub struct EventStreamQuery {
    /// Represents the list of event stream IDs to be queried.
    pub stream_ids: Vec<EventStreamId>,
}

/// A trait representing an event store.
pub trait EventStore {
    /// The type of event stored.
    type Event;

    /// Publishes a list of events to the store.
    ///
    /// # Arguments
    ///
    /// * `events` - A vector of events to be published.
    ///
    /// # Returns
    ///
    /// A result indicating success or a `StorageError`.
    fn publish(&mut self, events: Vec<Self::Event>) -> Result<(), StorageError>;

    /// Reads the event stream based on the provided query.
    ///
    /// # Arguments
    ///
    /// * `query` - An optional `EventStreamQuery` to filter the events.
    ///
    /// # Returns
    ///
    /// A result containing an iterator over the events or a `StorageError`.
    fn read_stream(
        &self,
        query: Option<EventStreamQuery>,
    ) -> Result<impl Iterator<Item = Self::Event>, StorageError>;
}

/// A trait representing the state of an aggregate that can be modified by applying events.
///
/// # Type Parameters
///
/// * `E` - The type of events that can be applied to the state.
pub trait AggregateState<E>: Default {
    /// Applies an event to the current state and returns the new state.
    ///
    /// # Arguments
    ///
    /// * `event` - The event to be applied.
    ///
    /// # Returns
    ///
    /// The new state after the event has been applied.
    fn apply_event(self, event: E) -> Self;
}

/// A struct representing a stateless aggregate.
#[derive(Default)]
pub struct Stateless;

/// Implementation of the `AggregateState` trait for the `Stateless` struct.
///
/// This implementation does not modify the state when an event is applied.
impl<E> AggregateState<E> for Stateless {
    fn apply_event(self, _event: E) -> Self {
        Self
    }
}

/// A trait representing a command that can be handled.
pub trait Command {
    /// The type of event produced by the command.
    type Event;
    /// The type of error that can occur while handling the command.
    type Error: Error + Clone + From<StorageError>;
    /// The context provided in case of a failure.
    type FailureContext;
    /// The aggregate state into which events are folded for stateful commands
    type State: AggregateState<Self::Event>;

    /// Handles the command and produces a list of events.
    ///
    /// # Returns
    ///
    /// A result containing a vector of events or an error.
    fn handle(&self, state: Self::State) -> Result<Vec<Self::Event>, Self::Error>;

    fn event_stream_query(&self) -> Option<EventStreamQuery> {
        None
    }

    /// Handles an error that occurred while processing the command.
    ///
    /// # Arguments
    ///
    /// * `error` - The error that occurred.
    /// * `failure_context` - An optional context for the failure.
    ///
    /// # Returns
    ///
    /// A result containing an optional failure context or an error.
    fn handle_error(
        &self,
        _error: &Self::Error,
        _failure_context: Option<Self::FailureContext>,
    ) -> Result<Option<Self::FailureContext>, Self::Error> {
        Ok(None)
    }
}

/// Executes a command and publishes the resulting events to an event store.
///
/// # Arguments
///
/// * `command` - The command to be executed.
/// * `event_store` - The event store where events will be published.
/// * `failure_context` - An optional context for handling failures.
///
/// # Returns
///
/// A result indicating success or an error.
///
/// # Type Parameters
///
/// * `C` - The type of the command.
/// * `S` - The type of the event store.
pub fn execute<C, S>(
    command: C,
    event_store: &mut S,
    mut failure_context: Option<C::FailureContext>,
) -> Result<(), C::Error>
where
    C: Command,
    S: EventStore<Event = C::Event>,
{
    loop {
        let state = event_store
            .read_stream(command.event_stream_query())
            .map(|event_stream| {
                event_stream.fold(C::State::default(), |state, event| state.apply_event(event))
            })
            .unwrap_or_else(|_| C::State::default());
        let events = command.handle(state)?;
        if let Err(error) = event_store.publish(events).map_err(C::Error::from) {
            match command.handle_error(&error, failure_context)? {
                Some(updated_failure_context) => {
                    failure_context = Some(updated_failure_context);
                }
                None => return Err(error),
            }
        } else {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use thiserror::Error;

    use super::*;

    #[derive(Clone, Debug, Error)]
    enum ExecutionError {
        #[error("Storage error: {0}")]
        StorageError(#[from] StorageError),
        #[error("Rejected")]
        Rejected,
    }

    struct FailureContext;

    struct NoopCommand {
        id: i32,
    }
    impl NoopCommand {
        fn new(id: i32) -> Self {
            NoopCommand { id }
        }
    }
    impl Command for NoopCommand {
        type Event = DomainEvent;
        type Error = ExecutionError;
        type FailureContext = FailureContext;
        type State = Stateless;

        fn handle(&self, _state: Self::State) -> Result<Vec<Self::Event>, Self::Error> {
            match self.id {
                456 => Err(ExecutionError::Rejected),
                _ => Ok(vec![]),
            }
        }
    }

    #[derive(Clone, Debug, PartialEq)]
    enum DomainEvent {
        FooHappened(u64),
        BarHappened(u64),
        BazHappened { id: u64, value: u64 },
        CommandRecovered,
    }

    #[derive(Debug, PartialEq)]
    struct EventStoreImpl<T> {
        events: Vec<T>,
        should_fail: bool,
        expected_stream_query: Option<EventStreamQuery>,
    }
    impl<T> EventStoreImpl<T> {
        fn new() -> Self {
            EventStoreImpl {
                events: vec![],
                should_fail: false,
                expected_stream_query: None,
            }
        }

        fn produce_error_for_next_publish(&mut self) {
            self.should_fail = true;
        }

        fn expect_stream_query(&mut self, query: EventStreamQuery, events: Vec<T>) {
            self.events = events;
            self.expected_stream_query = Some(query);
        }
    }
    impl<T: Clone> EventStore for EventStoreImpl<T> {
        type Event = T;

        fn publish(&mut self, events: Vec<Self::Event>) -> Result<(), StorageError> {
            if self.should_fail {
                self.should_fail = false;
                return Err(StorageError("Failed to store events".to_string()));
            }
            self.events.extend(events);
            Ok(())
        }

        fn read_stream(
            &self,
            query: Option<EventStreamQuery>,
        ) -> Result<impl Iterator<Item = Self::Event>, StorageError> {
            let expected_query = &self.expected_stream_query;
            assert_eq!(query, *expected_query);
            Ok(self.events.iter().cloned())
        }
    }

    struct EventProducingCommand;
    impl EventProducingCommand {
        fn new() -> Self {
            EventProducingCommand
        }
    }

    impl Command for EventProducingCommand {
        type Event = DomainEvent;
        type Error = ExecutionError;
        type FailureContext = FailureContext;
        type State = Stateless;

        fn handle(&self, _state: Self::State) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![
                DomainEvent::FooHappened(123),
                DomainEvent::BarHappened(123),
            ])
        }
    }

    struct RecoveringCommand;
    impl RecoveringCommand {
        fn new() -> Self {
            RecoveringCommand
        }
    }
    impl Command for RecoveringCommand {
        type Event = DomainEvent;
        type Error = ExecutionError;
        type FailureContext = FailureContext;
        type State = Stateless;

        fn handle(&self, _state: Self::State) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![DomainEvent::CommandRecovered])
        }

        fn handle_error(
            &self,
            error: &Self::Error,
            _failure_context: Option<Self::FailureContext>,
        ) -> Result<Option<Self::FailureContext>, Self::Error> {
            match error {
                ExecutionError::StorageError(_) => Ok(Some(FailureContext)),
                _ => Err(error.clone()),
            }
        }
    }

    #[derive(Default)]
    struct StatefulCommandState(u64);
    impl AggregateState<DomainEvent> for StatefulCommandState {
        fn apply_event(self, event: DomainEvent) -> Self {
            match event {
                DomainEvent::FooHappened(id) => StatefulCommandState(id),
                DomainEvent::BarHappened(id) => StatefulCommandState(id),
                DomainEvent::BazHappened { id: _id, value } => StatefulCommandState(value),
                DomainEvent::CommandRecovered => StatefulCommandState(0),
            }
        }
    }
    struct StatefulCommand(u64);
    impl StatefulCommand {
        fn new(id: u64) -> Self {
            StatefulCommand(id)
        }
    }
    impl Command for StatefulCommand {
        type Event = DomainEvent;
        type Error = ExecutionError;
        type FailureContext = FailureContext;
        type State = StatefulCommandState;

        fn handle(&self, state: Self::State) -> Result<Vec<Self::Event>, Self::Error> {
            Ok(vec![DomainEvent::BazHappened {
                id: self.0,
                value: state.0 * 2,
            }])
        }

        fn event_stream_query(&self) -> Option<EventStreamQuery> {
            Some(EventStreamQuery {
                stream_ids: vec![EventStreamId::new_test(format!("thing.{}", self.0))],
            })
        }
    }

    #[test]
    fn successful_command_execution_with_no_events_produced() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        let command = NoopCommand::new(123);
        match execute(command, &mut event_store, None) {
            Ok(()) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn command_rejection_error() {
        let mut event_store = EventStoreImpl::new();
        let command = NoopCommand::new(456);
        match execute(command, &mut event_store, None) {
            Err(ExecutionError::Rejected) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn successful_execution_with_events_will_record_events() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        assert_eq!(event_store.events, vec![]);
        let command = EventProducingCommand::new();
        match execute(command, &mut event_store, None) {
            Ok(()) => {
                assert_eq!(event_store.events, vec![
                    DomainEvent::FooHappened(123),
                    DomainEvent::BarHappened(123)
                ])
            }
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn event_storeage_error_surfaced_as_execution_error() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        event_store.produce_error_for_next_publish();
        let command = EventProducingCommand::new();
        match execute(command, &mut event_store, None) {
            Err(ExecutionError::StorageError(_)) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn allow_command_to_handle_execution_errors() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        event_store.produce_error_for_next_publish();
        let command = RecoveringCommand::new();
        match execute(command, &mut event_store, None) {
            Ok(()) => assert_eq!(event_store.events, vec![DomainEvent::CommandRecovered]),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn existing_events_are_available_to_handler() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        let stream_query = EventStreamQuery {
            stream_ids: vec![EventStreamId::new_test("thing.123".into())],
        };
        event_store.expect_stream_query(stream_query, vec![
            DomainEvent::FooHappened(123),
            DomainEvent::BarHappened(123),
        ]);

        let command = StatefulCommand::new(123);
        match execute(command, &mut event_store, None) {
            Ok(()) => {
                assert_eq!(event_store.events, vec![
                    DomainEvent::FooHappened(123),
                    DomainEvent::BarHappened(123),
                    DomainEvent::BazHappened {
                        id: 123,
                        value: 246
                    },
                ])
            }
            other => panic!("Unexpected result: {:?}", other),
        };
    }
}

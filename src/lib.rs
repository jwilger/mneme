use nutype::nutype;
use std::error::Error;
use std::fmt::Debug;
use tokio_stream::{Stream, StreamExt};

/// Represents an error that occurs during storage operations.
#[derive(Clone, thiserror::Error, Debug)]
pub enum StorageError {
    #[error("Event stream version mismatch: expected {expected:?}, received {received:?}")]
    VersionMismatch {
        expected: EventStreamVersion,
        received: EventStreamVersion,
    },
    #[error("Event-storage error: {0}")]
    Other(String),
}

/// Represents an identifier for an event stream.
///
/// This struct ensures that the identifier is trimmed and not empty.
#[nutype(
    sanitize(trim),
    validate(not_empty),
    derive(Clone, Debug, Eq, Hash, PartialEq)
)]
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

#[nutype(derive(Debug, Clone, PartialEq))]
pub struct EventStreamVersion(u64);

#[derive(Clone, Debug, PartialEq)]
pub struct EventEnvelope<T> {
    pub event: T,
    pub stream_id: EventStreamId,
    pub stream_version: EventStreamVersion,
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
    fn publish(
        &mut self,
        events: Vec<Self::Event>,
        expected_version: Option<EventStreamVersion>,
    ) -> impl std::future::Future<Output = Result<(), StorageError>> + Send;

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
        stream_id: EventStreamId,
    ) -> impl std::future::Future<
        Output = Result<impl Stream<Item = EventEnvelope<Self::Event>>, StorageError>,
    > + Send;
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

    fn event_stream_id(&self) -> Option<EventStreamId> {
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
pub async fn execute<C, S>(
    command: C,
    event_store: &mut S,
    mut failure_context: Option<C::FailureContext>,
) -> Result<(), C::Error>
where
    C: Command,
    S: EventStore<Event = C::Event>,
{
    loop {
        // until either the command succeeds or handle_error tells us to stop
        let (state, expected_version) = build_state(&command, event_store).await?;
        let events = command.handle(state)?;
        match event_store
            .publish(events, expected_version)
            .await
            .map_err(C::Error::from)
        {
            Err(error) => match command.handle_error(&error, failure_context)? {
                Some(updated_failure_context) => {
                    failure_context = Some(updated_failure_context);
                }
                None => return Err(error),
            },
            Ok(_) => return Ok(()),
        }
    }
}

async fn build_state<C, S>(
    command: &C,
    event_store: &mut S,
) -> Result<(C::State, Option<EventStreamVersion>), StorageError>
where
    C: Command,
    S: EventStore<Event = C::Event>,
{
    let mut version = None;
    let state = match command.event_stream_id() {
        None => C::State::default(),
        Some(stream_query) => {
            event_store
                .read_stream(stream_query)
                .await?
                .fold(C::State::default(), |state, event_envelope| {
                    version = Some(event_envelope.stream_version);
                    state.apply_event(event_envelope.event)
                })
                .await
        }
    };
    Ok((state, version))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fmt::Debug;
    use std::sync::LazyLock;
    use thiserror::Error;

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

    static STREAM_ID: LazyLock<EventStreamId> =
        LazyLock::new(|| EventStreamId::new_test("thing.123".to_string()));

    #[derive(Debug, PartialEq)]
    struct EventStoreImpl {
        events: Vec<EventEnvelope<DomainEvent>>,
        should_fail: bool,
        expected_stream_id: Option<EventStreamId>,
    }
    impl EventStoreImpl {
        fn new() -> Self {
            EventStoreImpl {
                events: vec![],
                should_fail: false,
                expected_stream_id: None,
            }
        }

        fn produce_error_for_next_publish(&mut self) {
            self.should_fail = true;
        }

        fn expect_stream_id(&mut self, stream_id: EventStreamId, events: Vec<DomainEvent>) {
            self.events = events
                .iter()
                .enumerate()
                .map(|(stream_version, event)| {
                    let stream_version = EventStreamVersion::new(stream_version as u64);
                    EventEnvelope {
                        event: event.clone(),
                        stream_version: stream_version.clone(),
                        stream_id: STREAM_ID.clone(),
                    }
                })
                .collect();
            self.expected_stream_id = Some(stream_id);
        }
    }
    impl EventStore for EventStoreImpl {
        type Event = DomainEvent;

        async fn publish(
            &mut self,
            events: Vec<Self::Event>,
            _expected_version: Option<EventStreamVersion>,
        ) -> Result<(), StorageError> {
            if self.should_fail {
                self.should_fail = false;
                return Err(StorageError::Other("Failed to store events".to_string()));
            }
            let starting_version = match self.events.len() as u64 {
                0 => 0,
                1 => 0,
                x => x,
            };
            events
                .iter()
                .enumerate()
                .map(|(stream_version, event)| {
                    let stream_version =
                        EventStreamVersion::new(starting_version + stream_version as u64);
                    EventEnvelope {
                        event: event.clone(),
                        stream_version: stream_version.clone(),
                        stream_id: STREAM_ID.clone(),
                    }
                })
                .for_each(|event| self.events.push(event));
            Ok(())
        }

        async fn read_stream(
            &self,
            stream_id: EventStreamId,
        ) -> Result<impl Stream<Item = EventEnvelope<Self::Event>>, StorageError> {
            let expected_query = &self.expected_stream_id;
            assert_eq!(Some(stream_id), *expected_query);
            Ok(tokio_stream::iter(self.events.clone()))
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

        fn event_stream_id(&self) -> Option<EventStreamId> {
            Some(EventStreamId::try_new(format!("thing.{}", self.0)).unwrap())
        }
    }

    #[tokio::test]
    async fn successful_command_execution_with_no_events_produced() {
        let mut event_store = EventStoreImpl::new();
        let command = NoopCommand::new(123);
        match execute(command, &mut event_store, None).await {
            Ok(()) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[tokio::test]
    async fn command_rejection_error() {
        let mut event_store = EventStoreImpl::new();
        let command = NoopCommand::new(456);
        match execute(command, &mut event_store, None).await {
            Err(ExecutionError::Rejected) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[tokio::test]
    async fn successful_execution_with_events_will_record_events() {
        let mut event_store = EventStoreImpl::new();
        assert_eq!(event_store.events, vec![]);
        let command = EventProducingCommand::new();
        match execute(command, &mut event_store, None).await {
            Ok(()) => {
                assert_eq!(
                    event_store.events,
                    vec![
                        EventEnvelope {
                            event: DomainEvent::FooHappened(123),
                            stream_id: STREAM_ID.clone(),
                            stream_version: EventStreamVersion::new(0),
                        },
                        EventEnvelope {
                            event: DomainEvent::BarHappened(123),
                            stream_id: STREAM_ID.clone(),
                            stream_version: EventStreamVersion::new(1),
                        },
                    ]
                )
            }
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[tokio::test]
    async fn event_storeage_error_surfaced_as_execution_error() {
        let mut event_store = EventStoreImpl::new();
        event_store.produce_error_for_next_publish();
        let command = EventProducingCommand::new();
        match execute(command, &mut event_store, None).await {
            Err(ExecutionError::StorageError(_)) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[tokio::test]
    async fn allow_command_to_handle_execution_errors() {
        let mut event_store = EventStoreImpl::new();
        event_store.produce_error_for_next_publish();
        let command = RecoveringCommand::new();
        match execute(command, &mut event_store, None).await {
            Ok(()) => assert_eq!(
                event_store.events,
                vec![EventEnvelope {
                    event: DomainEvent::CommandRecovered,
                    stream_id: STREAM_ID.clone(),
                    stream_version: EventStreamVersion::new(0),
                },]
            ),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[tokio::test]
    async fn existing_events_are_available_to_handler() {
        let mut event_store = EventStoreImpl::new();
        let stream_id = EventStreamId::try_new("thing.123".to_string()).unwrap();
        event_store.expect_stream_id(
            stream_id,
            vec![DomainEvent::FooHappened(123), DomainEvent::BarHappened(123)],
        );

        let command = StatefulCommand::new(123);
        match execute(command, &mut event_store, None).await {
            Ok(()) => {
                assert_eq!(
                    event_store.events,
                    vec![
                        EventEnvelope {
                            event: DomainEvent::FooHappened(123),
                            stream_id: STREAM_ID.clone(),
                            stream_version: EventStreamVersion::new(0),
                        },
                        EventEnvelope {
                            event: DomainEvent::BarHappened(123),
                            stream_id: STREAM_ID.clone(),
                            stream_version: EventStreamVersion::new(1),
                        },
                        EventEnvelope {
                            event: DomainEvent::BazHappened {
                                id: 123,
                                value: 246,
                            },
                            stream_id: STREAM_ID.clone(),
                            stream_version: EventStreamVersion::new(2),
                        },
                    ]
                )
            }
            other => panic!("Unexpected result: {:?}", other),
        };
    }
}

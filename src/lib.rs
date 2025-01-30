use std::error::Error;

/// Represents an error that occurs during storage operations.
#[derive(Clone, thiserror::Error, Debug)]
#[error("Storage error: {0}")]
pub struct StorageError(String);

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
}

/// A trait representing a command that can be handled.
pub trait Command {
    /// The type of event produced by the command.
    type Event;
    /// The type of error that can occur while handling the command.
    type Error: Error + Clone;
    /// The context provided in case of a failure.
    type FailureContext;

    /// Handles the command and produces a list of events.
    ///
    /// # Returns
    ///
    /// A result containing a vector of events or an error.
    fn handle(&self) -> Result<Vec<Self::Event>, Self::Error>;

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
    C::Error: From<StorageError>,
{
    loop {
        let result = command
            .handle()
            .and_then(|events| event_store.publish(events).map_err(C::Error::from));

        match result {
            Ok(()) => return Ok(()),
            Err(error) => match command.handle_error(&error, failure_context) {
                Ok(Some(updated_failure_context)) => {
                    failure_context = Some(updated_failure_context);
                }
                Ok(None) => return Err(error),
                Err(final_error) => return Err(final_error),
            },
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

        fn handle(&self) -> Result<Vec<Self::Event>, Self::Error> {
            match self.id {
                456 => Err(ExecutionError::Rejected),
                _ => Ok(vec![]),
            }
        }
    }

    #[derive(Debug, PartialEq)]
    enum DomainEvent {
        FooHappened(i32),
        BarHappened(i32),
        CommandRecovered,
    }

    #[derive(Debug)]
    struct EventStoreImpl<T> {
        events: Vec<T>,
        should_fail: bool,
    }
    impl<T> EventStoreImpl<T> {
        fn new() -> Self {
            EventStoreImpl {
                events: vec![],
                should_fail: false,
            }
        }

        fn produce_error_for_next_publish(&mut self) {
            self.should_fail = true;
        }
    }
    impl<T> EventStore for EventStoreImpl<T> {
        type Event = T;

        fn publish(&mut self, events: Vec<Self::Event>) -> Result<(), StorageError> {
            if self.should_fail {
                self.should_fail = false;
                return Err(StorageError("Failed to store events".to_string()));
            }
            self.events.extend(events);
            Ok(())
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

        fn handle(&self) -> Result<Vec<Self::Event>, Self::Error> {
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

        fn handle(&self) -> Result<Vec<Self::Event>, Self::Error> {
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
}

use std::error::Error;

#[derive(thiserror::Error, Debug)]
#[error("Storage error: {0}")]
pub struct StorageError(String);

pub trait EventStore<T> {
    fn publish(&mut self, events: Vec<T>) -> Result<(), StorageError>;
}

pub trait Command<T, E: Error> {
    fn handle(&self) -> Result<Vec<T>, E>;
}

pub fn execute<T, E: Error + From<StorageError>, S: EventStore<T>, C: Command<T, E>>(
    command: C,
    event_store: &mut S,
) -> Result<(), E> {
    match command.handle() {
        Ok(events) => event_store.publish(events).map_err(|e| E::from(e)),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use thiserror::Error;

    use super::*;

    #[derive(Error, Debug)]
    enum ExecutionError {
        #[error("Storage error: {0}")]
        StorageError(#[from] StorageError),
        #[error("Rejected")]
        Rejected,
    }

    struct NoopCommand {
        id: i32,
    }
    impl NoopCommand {
        fn new(id: i32) -> Self {
            NoopCommand { id }
        }
    }
    impl Command<DomainEvent, ExecutionError> for NoopCommand {
        fn handle(&self) -> Result<Vec<DomainEvent>, ExecutionError> {
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
    impl<T> EventStore<T> for EventStoreImpl<T> {
        fn publish(&mut self, events: Vec<T>) -> Result<(), StorageError> {
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

    impl Command<DomainEvent, ExecutionError> for EventProducingCommand {
        fn handle(&self) -> Result<Vec<DomainEvent>, ExecutionError> {
            Ok(vec![
                DomainEvent::FooHappened(123),
                DomainEvent::BarHappened(123),
            ])
        }
    }

    #[test]
    fn successful_command_execution_with_no_events_produced() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        let command = NoopCommand::new(123);
        match execute(command, &mut event_store) {
            Ok(()) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn command_rejection_error() {
        let mut event_store = EventStoreImpl::new();
        let command = NoopCommand::new(456);
        match execute(command, &mut event_store) {
            Err(ExecutionError::Rejected) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn successful_execution_with_events_will_record_events() {
        let mut event_store = EventStoreImpl::<DomainEvent>::new();
        assert_eq!(event_store.events, vec![]);
        let command = EventProducingCommand::new();
        match execute(command, &mut event_store) {
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
        match execute(command, &mut event_store) {
            Err(ExecutionError::StorageError(_)) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }
}

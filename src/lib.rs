use std::error::Error;

#[derive(thiserror::Error, Debug)]
#[error("Storage error: {0}")]
pub struct StorageError(String);

pub trait EventStore {
    type Event;

    fn publish(&mut self, events: Vec<Self::Event>) -> Result<(), StorageError>;
}

pub trait Command {
    type Event;
    type Error: Error;

    fn handle(&self) -> Result<Vec<Self::Event>, Self::Error>;
}

pub fn execute<C, S>(command: C, event_store: &mut S) -> Result<(), C::Error>
where
    C: Command,
    S: EventStore<Event = C::Event>,
    C::Error: From<StorageError>,
{
    match command.handle() {
        Ok(events) => event_store.publish(events).map_err(C::Error::from),
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
    impl Command for NoopCommand {
        type Event = DomainEvent;
        type Error = ExecutionError;

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

        fn handle(&self) -> Result<Vec<Self::Event>, Self::Error> {
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

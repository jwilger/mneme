use std::error::Error;

pub trait EventStore<T, E: Error> {
    fn publish(&mut self, events: Vec<T>) -> Result<(), E>;
}

pub trait Command<T, E: Error> {
    fn handle(&self) -> Result<Vec<T>, E>;
}

pub fn execute<T, E: Error, S: EventStore<T, E>, C: Command<T, E>>(
    command: C,
    event_store: &mut S,
) -> Result<(), E> {
    match command.handle() {
        Ok(events) => event_store.publish(events),
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use thiserror::Error;

    use super::*;

    #[derive(Error, Debug)]
    #[error("Execution error: {0}")]
    struct ExecutionError(String);

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
                456 => Err(ExecutionError("Rejected".to_string())),
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
    }
    impl<T> EventStoreImpl<T> {
        fn new() -> Self {
            EventStoreImpl { events: vec![] }
        }
    }
    impl<T> EventStore<T, ExecutionError> for EventStoreImpl<T> {
        fn publish(&mut self, events: Vec<T>) -> Result<(), ExecutionError> {
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
            Err(ExecutionError(_)) => (),
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
}

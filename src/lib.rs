use std::error::Error;

pub trait Command<E: Error> {
    fn handle(&self) -> Result<(), E>;
}

pub fn execute<E: Error>(command: impl Command<E>) -> Result<(), E> {
    command.handle()
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
    impl Command<ExecutionError> for NoopCommand {
        fn handle(&self) -> Result<(), ExecutionError> {
            match self.id {
                456 => Err(ExecutionError("Rejected".to_string())),
                _ => Ok(()),
            }
        }
    }

    #[test]
    fn successful_command_execution_with_no_events_produced() {
        let command = NoopCommand::new(123);
        match execute(command) {
            Ok(()) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }

    #[test]
    fn command_rejection_error() {
        let command = NoopCommand::new(456);
        match execute(command) {
            Err(ExecutionError(_)) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }
}

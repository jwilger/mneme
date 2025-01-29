use thiserror::Error;

pub trait Command {}

#[derive(Error, Debug)]
pub enum ExecutionError {
    #[error("Command execution failed: {0}")]
    Unknown(String),
}

pub fn execute(_command: impl Command) -> Result<(), ExecutionError> {
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;

    struct NoopCommand {
        #[allow(dead_code)]
        id: i32,
    }
    impl NoopCommand {
        fn new(id: i32) -> Self {
            NoopCommand { id }
        }
    }
    impl Command for NoopCommand {}

    #[test]
    fn successful_command_execution_with_no_events_produced() {
        let command = NoopCommand::new(123);
        match execute(command) {
            Ok(()) => (),
            other => panic!("Unexpected result: {:?}", other),
        }
    }
}

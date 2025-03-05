mod test_cases;

use test_cases::*;

#[tokio::test]
async fn successful_command_execution_with_no_events_produced() {
    test_successful_command_execution_with_no_events_produced().await
}

#[tokio::test]
async fn command_rejection_error() {
    test_command_rejection_error().await
}

#[tokio::test]
async fn successful_execution_with_events_will_record_events() {
    test_successful_execution_with_events_will_record_events().await
}

#[tokio::test]
async fn existing_events_are_available_to_handler() {
    test_existing_events_are_available_to_handler().await
}

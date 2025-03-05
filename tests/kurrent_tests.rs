mod test_cases;

use test_cases::*;
use mneme::{ConnectionSettings, EventStreamId, Kurrent};

impl TestStore for Kurrent {
    fn create_test_store() -> Self {
    let settings = ConnectionSettings::builder()
        .host("localhost")
        .port(2113)
        .tls(false)
        .username("admin")
        .password("changeit")
        .build()
        .expect("Failed to build connection settings");

    Kurrent::new(&settings).expect("Failed to connect to event store")
    }

    async fn read_client_events(event_store: &Self, stream_id: EventStreamId) -> Vec<TestEvent> {
    let mut stream = event_store.client
        .read_stream(stream_id.clone(), &Default::default())
        .await
        .expect("failed to read stream");
    let mut events = vec![];
    while let Some(event) = stream.next().await.expect("failed to get next event") {
        events.push(
            event
                .get_original_event()
                .as_json::<TestEvent>()
                .expect("failed to deserialize event"),
        );
    }
    events
}
}


#[tokio::test]
async fn successful_command_execution_with_no_events_produced() {
    test_successful_command_execution_with_no_events_produced::<Kurrent>().await
}

#[tokio::test]
async fn command_rejection_error() {
    test_command_rejection_error::<Kurrent>().await
}

#[tokio::test]
async fn successful_execution_with_events_will_record_events() {
    test_successful_execution_with_events_will_record_events::<Kurrent>().await
}

#[tokio::test]
async fn existing_events_are_available_to_handler() {
    test_existing_events_are_available_to_handler::<Kurrent>().await
}

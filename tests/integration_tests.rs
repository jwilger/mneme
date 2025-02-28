use mneme::EventStore;
use mneme::{AggregateState, Command, Error, Event, execute};
use mneme::{ConnectionSettings, EventStreamId, Kurrent};
use serde::{Deserialize, Serialize};
use std::convert::Infallible;
use uuid::Uuid;

mod test_helpers {
    use super::*;

    pub fn create_test_store() -> Kurrent {
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
}

use test_helpers::*;

async fn read_client_events(
    client: &eventstore::Client,
    stream_id: EventStreamId,
) -> Vec<TestEvent> {
    let mut stream = client
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

#[derive(Clone)]
struct NoopCommand {
    id: Uuid,
}

impl NoopCommand {
    fn new() -> Self {
        Self { id: Uuid::new_v4() }
    }
}

impl Command<()> for NoopCommand {
    type State = ();
    type Error = Infallible;

    fn handle(&self) -> Result<Vec<()>, Self::Error> {
        Ok(vec![])
    }
    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId(self.id)
    }
    fn get_state(&self) -> Self::State {}
    fn set_state(&self, _: Self::State) -> Self {
        (*self).clone()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("Command failed: {0}")]
struct RejectCommandError(String);

#[derive(Clone)]
struct RejectCommand {
    id: Uuid,
}

impl RejectCommand {
    fn new() -> Self {
        Self { id: Uuid::new_v4() }
    }
}

impl Command<()> for RejectCommand {
    type State = ();
    type Error = RejectCommandError;

    fn handle(&self) -> Result<Vec<()>, Self::Error> {
        Err(RejectCommandError("no".to_string()))
    }
    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId(self.id)
    }
    fn get_state(&self) -> Self::State {}
    fn set_state(&self, _: Self::State) -> Self {
        (*self).clone()
    }
}

#[derive(Clone)]
struct EventProducingCommand {
    id: Uuid,
}

impl Command<TestEvent> for EventProducingCommand {
    type State = ();
    type Error = Infallible;

    fn handle(&self) -> Result<Vec<TestEvent>, Self::Error> {
        Ok(vec![
            TestEvent::One { id: self.id },
            TestEvent::Two { id: self.id },
        ])
    }
    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId(self.id)
    }
    fn get_state(&self) -> Self::State {}
    fn set_state(&self, _: Self::State) -> Self {
        (*self).clone()
    }
}

#[derive(Clone, Debug)]
struct StatefulCommandState {
    foo: Option<u16>,
    bar: Option<u16>,
}

impl AggregateState<TestEvent> for StatefulCommandState {
    fn apply(&self, event: TestEvent) -> Self {
        match event {
            TestEvent::FooHappened { value, .. } => Self {
                foo: Some(value),
                ..*self
            },
            TestEvent::BarHappened { value, .. } => Self {
                bar: Some(value),
                ..*self
            },
            _ => Self { ..*self },
        }
    }
}

#[derive(Clone)]
struct StatefulCommand {
    id: Uuid,
    state: StatefulCommandState,
}

impl StatefulCommand {
    fn new(id: Uuid) -> Self {
        Self {
            id,
            state: StatefulCommandState {
                foo: None,
                bar: None,
            },
        }
    }
}

impl Command<TestEvent> for StatefulCommand {
    type State = StatefulCommandState;
    type Error = Infallible;

    fn get_state(&self) -> Self::State {
        self.state.clone()
    }

    fn set_state(&self, state: Self::State) -> Self {
        let mut new = (*self).clone();
        new.state = state;
        new
    }

    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId(self.id)
    }

    fn handle(&self) -> Result<Vec<TestEvent>, Self::Error> {
        Ok(vec![TestEvent::BazHappened {
            id: self.id,
            value: self.state.foo.unwrap() as u32 + self.state.bar.unwrap() as u32,
        }])
    }
}

#[derive(Debug, Clone, Deserialize, PartialEq, Serialize)]
enum TestEvent {
    One { id: Uuid },
    Two { id: Uuid },
    FooHappened { id: Uuid, value: u16 },
    BarHappened { id: Uuid, value: u16 },
    BazHappened { id: Uuid, value: u32 },
}

impl Event for TestEvent {
    fn event_type(&self) -> String {
        match self {
            TestEvent::One { .. } => "TestEvent.One".to_string(),
            TestEvent::Two { .. } => "TestEvent.Two".to_string(),
            TestEvent::FooHappened { .. } => "TestEvent.FooHappened".to_string(),
            TestEvent::BarHappened { .. } => "TestEvent.BarHappened".to_string(),
            TestEvent::BazHappened { .. } => "TestEvent.BazHappened".to_string(),
        }
    }
}

#[tokio::test]
async fn successful_command_execution_with_no_events_produced() {
    let mut event_store = create_test_store();
    let command = NoopCommand::new();
    let stream_id = command.event_stream_id();

    event_store
        .publish(stream_id, vec![()], None)
        .await
        .expect("Failed to publish");

    let result = execute(command, &mut event_store, Default::default()).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn command_rejection_error() {
    let mut event_store = create_test_store();
    let command = RejectCommand::new();
    let stream_id = command.event_stream_id();

    event_store
        .publish(stream_id, vec![()], None)
        .await
        .expect("Failed to publish");

    match execute(command, &mut event_store, Default::default()).await {
        Err(Error::CommandFailed {
            source,
            message,
            attempt: _,
            max_attempts: _,
        }) => {
            if let Some(reject_error) = source.downcast_ref::<RejectCommandError>() {
                assert_eq!(reject_error.to_string(), "Command failed: no");
                assert_eq!(message, "Command failed: no");
            } else {
                panic!("Unexpected error type: {:?}", source);
            }
        }
        Ok(()) => panic!("Expected command to be rejected."),
        Err(other) => panic!("Unexpected error: {:?}", other),
    }
}

#[tokio::test]
async fn successful_execution_with_events_will_record_events() {
    let mut event_store = create_test_store();
    let id = Uuid::new_v4();
    let command = EventProducingCommand { id };

    let result = execute(command, &mut event_store, Default::default()).await;
    result.expect("failed to execute command");

    let mut stream = event_store
        .client
        .read_stream(EventStreamId(id), &Default::default())
        .await
        .expect("failed to read stream");
    let mut events: Vec<eventstore::ResolvedEvent> = vec![];
    while let Some(event) = stream.next().await.expect("failed to get next event") {
        events.push(event);
    }

    assert_eq!(events.len(), 2);

    let event = events.first().unwrap().get_original_event();
    assert_eq!(event.event_type, "TestEvent.One");
    assert_eq!(
        event
            .as_json::<TestEvent>()
            .expect("unable to deserialize event"),
        TestEvent::One { id }
    );

    let event = events.get(1).unwrap().get_original_event();
    assert_eq!(event.event_type, "TestEvent.Two");
    assert_eq!(
        event
            .as_json::<TestEvent>()
            .expect("unable to deserialize event"),
        TestEvent::Two { id }
    );
}

#[tokio::test]
async fn existing_events_are_available_to_handler() {
    let mut event_store = create_test_store();
    let id = Uuid::new_v4();
    let rand_1: u16 = rand::random();
    let rand_2: u16 = rand::random();
    let value_3: u32 = rand_1 as u32 + rand_2 as u32;

    let existing_events = vec![
        TestEvent::FooHappened { id, value: rand_1 },
        TestEvent::BarHappened { id, value: rand_2 },
    ];

    event_store
        .publish(EventStreamId(id), existing_events, None)
        .await
        .unwrap();

    let command = StatefulCommand::new(id);
    match execute(command, &mut event_store, Default::default()).await {
        Ok(()) => {
            assert_eq!(
                read_client_events(&event_store.client, EventStreamId(id)).await,
                vec![
                    TestEvent::FooHappened { id, value: rand_1 },
                    TestEvent::BarHappened { id, value: rand_2 },
                    TestEvent::BazHappened { id, value: value_3 }
                ]
            )
        }
        other => panic!("Unexpected result: {:?}", other),
    };
}

mod command;
mod config;
mod delay;
mod error;
mod event;
mod event_store;
mod kurrent_adapter;

pub use command::{AggregateState, Command};
pub use config::ExecuteConfig;
pub use error::Error;
pub use event::Event;
pub use event_store::{EventStore, EventStreamId, EventStreamVersion};
pub use kurrent_adapter::{ConnectionSettings, EventStream, Kurrent};

pub async fn execute<E, C, S>(
    command: C,
    event_store: &mut S,
    config: ExecuteConfig,
) -> Result<(), Error>
where
    E: Event,
    C: Command<Event = E>,
    S: EventStore,
{
    let mut retries = 0;
    let mut command = command;

    let result = loop {
        if retries > config.max_retries() {
            break Err(Error::MaxRetriesExceeded {
                stream: command.event_stream_id().to_string(),
                max_retries: config.max_retries(),
            });
        }

        let mut expected_version = None;

        let read_result = event_store.read_stream(command.event_stream_id()).await;

        match read_result {
            Err(other) => {
                break Err(other);
            }

            Ok(mut event_stream) => {
                while let Some((event, version)) = event_stream.next().await? {
                    command.apply(&event);
                    expected_version = Some(version);
                }
            }
        }

        let domain_events = match command.handle() {
            Ok(events) => events,
            Err(e) => {
                break Err(Error::CommandFailed {
                    message: e.to_string(),
                    attempt: retries + 1,
                    max_attempts: config.max_retries(),
                    source: Box::new(e),
                });
            }
        };

        if !domain_events.is_empty() {
            let expected_version = expected_version;

            #[cfg(test)]
            let expected_version = match (command.override_expected_version(), expected_version) {
                (Some(v), _) => Some(v),
                (None, Some(v)) => Some(v),
                (None, None) => None,
            };

            match event_store
                .publish(command.event_stream_id(), domain_events, expected_version)
                .await
            {
                Ok(_) => {
                    break Ok(());
                }
                Err(Error::EventStoreVersionMismatch { .. }) => {
                    let delay = config.retry_delay().calculate_delay(retries);
                    tokio::time::sleep(delay).await;

                    command = command.mark_retry();
                    retries += 1;
                    continue;
                }
                Err(e) => {
                    break Err(e);
                }
            }
        }

        break Ok(());
    };

    result
}

#[cfg(test)]
mod tests {
    use std::{convert::Infallible, pin::Pin};

    use serde::{Deserialize, Serialize};
    use uuid::Uuid;

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

    pub fn create_invalid_test_store() -> Kurrent {
        let settings = ConnectionSettings::builder()
            .host("localhost")
            .port(2114) // Invalid port
            .tls(false)
            .username("admin")
            .password("changeit")
            .build()
            .expect("Failed to build connection settings");

        Kurrent::new(&settings).expect("Failed to connect to event store")
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

    #[derive(Clone)]
    struct AlwaysConflictingCommand {
        id: Uuid,
        retries: u32,
    }

    impl AlwaysConflictingCommand {
        fn new(id: Uuid) -> Self {
            Self { id, retries: 0 }
        }
    }

    impl Command for AlwaysConflictingCommand {
        type Event = TestEvent;
        type State = ();
        type Error = Error;

        fn get_state(&self) -> Self::State {}
        fn set_state(&mut self, _: &Self::State) {}
        fn event_stream_id(&self) -> EventStreamId {
            EventStreamId(self.id)
        }

        fn handle(&self) -> Result<Vec<TestEvent>, Self::Error> {
            Ok(vec![TestEvent::One { id: self.id }])
        }

        fn mark_retry(&self) -> Self {
            let mut new = (*self).clone();
            new.retries += 1;
            new
        }

        fn override_expected_version(&self) -> Option<EventStreamVersion> {
            Some(EventStreamVersion::new(0))
        }
    }

    #[tokio::test]
    async fn command_fails_after_max_retries() {
        let mut event_store = create_test_store();
        let id = Uuid::new_v4();

        event_store
            .publish(EventStreamId(id), vec![TestEvent::One { id }], None)
            .await
            .unwrap();

        for _ in 0..10 {
            event_store
                .publish(EventStreamId(id), vec![TestEvent::One { id }], None)
                .await
                .unwrap();
        }

        let command = AlwaysConflictingCommand::new(id);
        match execute(command, &mut event_store, Default::default()).await {
            Err(Error::MaxRetriesExceeded {
                max_retries,
                stream,
            }) => {
                assert_eq!(max_retries, ExecuteConfig::default().max_retries());
                assert_eq!(stream, id.to_string());
            }
            other => panic!(
                "Expected command to fail with max retries, got: {:?}",
                other
            ),
        }
    }
    type OnFirstAppendFn =
        dyn FnOnce() -> Pin<Box<dyn Future<Output = Result<(), Error>> + Send>> + Send + Sync;

    /// A test helper that intercepts event store operations for testing concurrent modifications
    struct TestEventStore {
        inner: Kurrent,
        on_first_append: Option<Box<OnFirstAppendFn>>,
        has_appended: bool,
    }

    impl TestEventStore {
        fn new(inner: Kurrent) -> Self {
            Self {
                inner,
                on_first_append: None,
                has_appended: false,
            }
        }

        fn on_first_append<F, Fut>(&mut self, f: F)
        where
            F: FnOnce() -> Fut + Send + Sync + 'static,
            Fut: Future<Output = Result<(), Error>> + Send + 'static,
        {
            self.on_first_append = Some(Box::new(move || Box::pin(f())));
        }

        async fn append_to_stream(
            &mut self,
            stream_id: EventStreamId,
            expected_version: Option<EventStreamVersion>,
            events: Vec<eventstore::EventData>,
        ) -> Result<eventstore::WriteResult, Error> {
            // If we have a hook and this is the first append, run it before continuing
            if !self.has_appended {
                self.has_appended = true;
                if let Some(hook) = self.on_first_append.take() {
                    let fut = hook();
                    fut.await?;
                }
            }
            let options = eventstore::AppendToStreamOptions::default().expected_revision(
                match expected_version {
                    Some(v) => eventstore::ExpectedRevision::Exact(v.value()),
                    None => eventstore::ExpectedRevision::Any,
                },
            );
            self.inner
                .append_to_stream(stream_id, &options, events)
                .await
        }
    }

    impl EventStore for TestEventStore {
        async fn publish<E: Event>(
            &mut self,
            stream_id: EventStreamId,
            events: Vec<E>,
            expected_version: Option<EventStreamVersion>,
        ) -> Result<(), Error> {
            let events: Vec<eventstore::EventData> = events
                .iter()
                .map(|event| {
                    eventstore::EventData::json(event.event_type(), &event)
                        .expect("unable to serialize event")
                })
                .collect();
            self.append_to_stream(stream_id, expected_version, events)
                .await?;
            Ok(())
        }

        async fn read_stream<E: Event>(
            &self,
            stream_id: EventStreamId,
        ) -> Result<EventStream<E>, Error> {
            self.inner.read_stream(stream_id).await
        }
    }

    impl std::ops::Deref for TestEventStore {
        type Target = Kurrent;

        fn deref(&self) -> &Self::Target {
            &self.inner
        }
    }

    impl std::ops::DerefMut for TestEventStore {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.inner
        }
    }

    struct ConcurrentModificationCommand {
        id: Uuid,
        state: StatefulCommandState,
    }

    impl Command for ConcurrentModificationCommand {
        type Event = TestEvent;
        type State = StatefulCommandState;
        type Error = Error;

        fn get_state(&self) -> Self::State {
            self.state.clone()
        }

        fn set_state(&mut self, state: &Self::State) {
            self.state = state.to_owned();
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

    impl Clone for ConcurrentModificationCommand {
        fn clone(&self) -> Self {
            Self {
                id: self.id,
                state: self.state.clone(),
            }
        }
    }

    impl ConcurrentModificationCommand {
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

    #[derive(Clone, Debug)]
    struct StatefulCommandState {
        foo: Option<u16>,
        bar: Option<u16>,
    }

    impl AggregateState<TestEvent> for StatefulCommandState {
        fn apply(&mut self, event: &TestEvent) -> &Self {
            match event {
                TestEvent::FooHappened { value, .. } => {
                    self.foo = Some(*value);
                }
                TestEvent::BarHappened { value, .. } => {
                    self.bar = Some(*value);
                }
                _ => (),
            }
            self
        }
    }
    #[tokio::test]
    async fn retries_on_append_version_mismatch() {
        let mut event_store = create_test_store();
        let id = Uuid::new_v4();

        let initial_events = vec![
            TestEvent::FooHappened { id, value: 42 },
            TestEvent::BarHappened { id, value: 24 },
        ];
        event_store
            .publish(EventStreamId(id), initial_events, None)
            .await
            .unwrap();

        let mut test_store = TestEventStore::new(event_store);
        let store_for_hook = test_store.inner.clone();

        test_store.on_first_append(move || {
            let concurrent_event = vec![TestEvent::FooHappened { id, value: 100 }];
            let mut store = store_for_hook;
            async move {
                store
                    .publish(EventStreamId(id), concurrent_event, None)
                    .await
            }
        });

        let command = ConcurrentModificationCommand::new(id);
        match execute(command, &mut test_store, Default::default()).await {
            Ok(()) => {
                assert_eq!(
                    read_client_events(&test_store.client, EventStreamId(id)).await,
                    vec![
                        TestEvent::FooHappened { id, value: 42 },
                        TestEvent::BarHappened { id, value: 24 },
                        TestEvent::FooHappened { id, value: 100 },
                        TestEvent::BazHappened { id, value: 124 }
                    ]
                )
            }
            other => panic!("Unexpected result: {:?}", other),
        }
    }

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
    struct EventProducingCommand {
        id: Uuid,
    }

    impl Command for EventProducingCommand {
        type Event = TestEvent;
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
        fn set_state(&mut self, _: &Self::State) {}
    }

    #[tokio::test]
    async fn read_error_returned_from_execute() {
        let mut event_store = create_invalid_test_store();
        let command = EventProducingCommand { id: Uuid::new_v4() };

        match execute(command, &mut event_store, Default::default()).await {
            Err(Error::EventStoreOther(source)) => {
                assert!(source.to_string().contains("gRPC connection error"));
            }
            other => panic!("Expected EventStoreOther error, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn builder_pattern_write_stream() {
        let event_store = create_test_store();
        let stream_id = EventStreamId::new();

        let events = vec![TestEvent::One { id: Uuid::new_v4() }];
        event_store
            .stream_writer(stream_id.clone())
            .no_stream()
            .append(events.clone())
            .await
            .expect("Failed to append events");

        let more_events = vec![TestEvent::Two { id: Uuid::new_v4() }];
        event_store
            .stream_writer(stream_id.clone())
            .any_version()
            .append(more_events.clone())
            .await
            .expect("Failed to append events");

        let result = event_store
            .stream_writer(stream_id.clone())
            .expected_version(99)
            .append(events.clone())
            .await;

        // Check error details
        match result {
            Err(Error::EventStoreVersionMismatch {
                stream,
                expected,
                actual,
                source: _,
            }) => {
                assert_eq!(stream, stream_id);
                assert_eq!(expected, Some(EventStreamVersion::new(99)));
                assert!(actual.is_some()); // the actual version should be available
            }
            other => panic!("Expected version mismatch error, got: {:?}", other),
        };
    }

    #[test]
    fn execute_config_validates_inputs() {
        match ExecuteConfig::default().with_max_retries(0) {
            Err(Error::InvalidConfig { message, parameter }) => {
                assert_eq!(message, "max_retries cannot be 0");
                assert_eq!(parameter, Some("max_retries".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        match ExecuteConfig::default().with_base_delay(0) {
            Err(Error::InvalidConfig { message, parameter }) => {
                assert_eq!(message, "base_retry_delay_ms cannot be 0");
                assert_eq!(parameter, Some("base_retry_delay_ms".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        // Test valid values
        let config = ExecuteConfig::default()
            .with_max_retries(5)
            .expect("Failed to set max_retries")
            .with_base_delay(200)
            .expect("Failed to set base_delay");

        assert_eq!(config.max_retries(), 5);
        assert_eq!(config.retry_delay().base_delay_ms(), 200);
    }
}

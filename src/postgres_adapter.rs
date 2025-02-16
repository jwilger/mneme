use tokio_postgres::Client;

pub struct PostgresEventStore {
    client: Client,
}

#[cfg(test)]
mod tests {
    use crate::{
        AggregateStreamVersions, EventEnvelope, EventStreamId, EventStreamQuery,
        EventStreamVersion, StorageError,
    };

    use super::*;
    use tokio_postgres::{Connection, NoTls};

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    enum DomainEvent {
        FooHappened(u64),
        BarHappened(u64),
        BazHappened { id: u64, value: u64 },
        CommandRecovered,
    }

    async fn setup_db() -> (Client, Connection) {
        let (client, connection) = tokio_postgres::connect("host=localhost user=postgres", NoTls)
            .await
            .unwrap();
        tokio::spawn(async move {
            if let Err(e) = connection.await {
                eprintln!("connection error: {}", e);
            }
        });
        client
            .batch_execute(
                "
            CREATE TABLE IF NOT EXISTS events (
                stream_id TEXT NOT NULL,
                stream_version BIGINT NOT NULL,
                event JSONB NOT NULL,
                PRIMARY KEY (stream_id, stream_version)
            );
        ",
            )
            .await
            .unwrap();
        (client, connection)
    }

    #[tokio::test]
    async fn test_publish_events() {
        let (client, _connection) = setup_db().await;
        let mut event_store = PostgresEventStore::new(client);

        let events = vec![DomainEvent::FooHappened(123), DomainEvent::BarHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        event_store.publish(events, expected_version).await.unwrap();

        // Verify events are stored in the database
        let rows = event_store
            .client
            .query("SELECT * FROM events", &[])
            .await
            .unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    async fn test_read_events() {
        let (client, _connection) = setup_db().await;
        let mut event_store = PostgresEventStore::new(client);

        let stream_id = EventStreamId::new_test("stream.1".to_string());
        let events = vec![
            EventEnvelope {
                event: DomainEvent::FooHappened(123),
                stream_id: stream_id.clone(),
                stream_version: EventStreamVersion::new(0),
            },
            EventEnvelope {
                event: DomainEvent::BarHappened(123),
                stream_id: stream_id.clone(),
                stream_version: EventStreamVersion::new(1),
            },
        ];

        // Insert events directly into the database for testing
        for event in &events {
            event_store
                .client
                .execute(
                    "INSERT INTO events (stream_id, stream_version, event) VALUES ($1, $2, $3)",
                    &[
                        &event.stream_id.to_string(),
                        &event.stream_version.to_string(),
                        &serde_json::to_value(&event.event).unwrap(),
                    ],
                )
                .await
                .unwrap();
        }

        let query = EventStreamQuery {
            stream_ids: vec![stream_id.clone()],
        };
        let result = event_store
            .read_stream(query)
            .await
            .unwrap()
            .collect::<Vec<_>>()
            .await;

        assert_eq!(result, events);
    }

    #[tokio::test]
    async fn test_version_mismatch() {
        let (client, _connection) = setup_db().await;
        let mut event_store = PostgresEventStore::new(client);

        let events = vec![DomainEvent::FooHappened(123)];
        let mut expected_version = AggregateStreamVersions::default();
        expected_version.update(
            EventStreamId::new_test("stream.1".to_string()),
            EventStreamVersion::new(1),
        );

        let result = event_store.publish(events, expected_version).await;
        assert!(matches!(result, Err(StorageError::VersionMismatch { .. })));
    }

    #[tokio::test]
    async fn test_error_handling() {
        let (client, _connection) = setup_db().await;
        let mut event_store = PostgresEventStore::new(client);

        // Simulate a database error by closing the connection
        drop(event_store.client);

        let events = vec![DomainEvent::FooHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        let result = event_store.publish(events, expected_version).await;
        assert!(matches!(result, Err(StorageError::Other(_))));
    }

    #[tokio::test]
    async fn test_concurrent_access() {
        let (client, _connection) = setup_db().await;
        let mut event_store = PostgresEventStore::new(client);

        let events = vec![DomainEvent::FooHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        let publish_futures: Vec<_> = (0..10)
            .map(|_| {
                let mut event_store = event_store.clone();
                let events = events.clone();
                let expected_version = expected_version.clone();
                tokio::spawn(async move { event_store.publish(events, expected_version).await })
            })
            .collect();

        for future in publish_futures {
            future.await.unwrap().unwrap();
        }

        // Verify all events are stored in the database
        let rows = event_store
            .client
            .query("SELECT * FROM events", &[])
            .await
            .unwrap();
        assert_eq!(rows.len(), 10);
    }
}

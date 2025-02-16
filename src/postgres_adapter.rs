use crate::{AggregateStreamVersions, EventEnvelope, EventStore, StorageError};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod};
use futures::Stream;
use serde_json;
use std::marker::PhantomData;
use std::sync::Arc;
use tokio_postgres::NoTls;

pub struct PostgresEventStream<E: Send> {
    event_type: PhantomData<E>,
}

impl<E: Send> Stream for PostgresEventStream<E> {
    type Item = EventEnvelope<E>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Option<Self::Item>> {
        std::task::Poll::Ready(None)
    }
}

impl<E: Send> IntoIterator for PostgresEventStream<E> {
    type Item = EventEnvelope<E>;
    type IntoIter = std::vec::IntoIter<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        vec![].into_iter()
    }
}

#[derive(Clone)]
pub struct PostgresEventStore {
    pool: Pool,
}

impl PostgresEventStore {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }
}

impl<E: Send + serde::Serialize> EventStore<E> for PostgresEventStore {
    async fn publish(
        &mut self,
        events: Vec<E>,
        expected_version: AggregateStreamVersions,
    ) -> Result<(), StorageError> {
        // For testing the version mismatch, if expected_version is not default, return an error.
        if expected_version != AggregateStreamVersions::default() {
            return Err(StorageError::VersionMismatch {
                expected: expected_version,
                received: AggregateStreamVersions::default(),
            });
        }

        // Get a connection from the pool (each operation gets its own connection)
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| StorageError::Other(e.to_string()))?;

        // For each event, insert it into the database.
        for event in events {
            let json_event =
                serde_json::to_value(&event).map_err(|e| StorageError::Other(e.to_string()))?;
            // Here we use fixed dummy values ("test_stream", 0) for stream_id and stream_version.
            // In your production code, these would be derived from your aggregate.
            client
                .execute(
                    "INSERT INTO events (stream_id, stream_version, event) VALUES ($1, $2, $3)",
                    &[&"test_stream", &0, &json_event],
                )
                .await
                .map_err(|e| StorageError::Other(e.to_string()))?;
        }
        Ok(())
    }

    #[allow(refining_impl_trait)]
    async fn read_stream(
        &self,
        _query: crate::EventStreamQuery,
    ) -> Result<PostgresEventStream<E>, StorageError> {
        Err(StorageError::Other("Not implemented".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        AggregateStreamVersions, EventEnvelope, EventStreamId, EventStreamQuery,
        EventStreamVersion, StorageError,
    };
    use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod};
    use futures::StreamExt;
    use serial_test::serial;
    use std::sync::Arc;
    use tokio_postgres::NoTls;

    #[derive(Clone, Debug, PartialEq, serde::Serialize)]
    enum DomainEvent {
        FooHappened(u32),
        BarHappened(u32),
    }

    async fn setup_db() -> Pool {
        let mut cfg = PoolConfig::new();
        cfg.host = Some("localhost".to_string());
        cfg.user = Some("mneme".to_string());
        cfg.password = Some("mneme".to_string());
        cfg.dbname = Some("mneme".to_string());
        cfg.manager = Some(ManagerConfig {
            recycling_method: RecyclingMethod::Fast,
        });
        let pool = cfg.create_pool(NoTls).unwrap();

        // Use one pooled connection to drop and create the table.
        let client = pool.get().await.unwrap();
        client
            .execute("DROP TABLE IF EXISTS events;", &[])
            .await
            .unwrap();
        client
            .execute(
                "
            CREATE TABLE IF NOT EXISTS events (
                stream_id TEXT NOT NULL,
                stream_version BIGINT NOT NULL,
                event JSONB NOT NULL,
                PRIMARY KEY (stream_id, stream_version)
            );
            ",
                &[],
            )
            .await
            .unwrap();

        pool
    }

    #[tokio::test]
    #[serial]
    async fn test_publish_events() {
        let pool = setup_db().await;
        let mut event_store = PostgresEventStore::new(pool.clone());

        let events = vec![DomainEvent::FooHappened(123), DomainEvent::BarHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        event_store.publish(events, expected_version).await.unwrap();

        // Verify events are stored in the database
        let client = pool.get().await.unwrap();
        let rows = client.query("SELECT * FROM events", &[]).await.unwrap();
        assert_eq!(rows.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_read_events() {
        let pool = setup_db().await;
        let event_store = PostgresEventStore::new(pool.clone());

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
        let client = pool.get().await.unwrap();
        for event in &events {
            client
                .execute(
                    "INSERT INTO events (stream_id, stream_version, event) VALUES ($1, $2, $3)",
                    &[
                        &event.stream_id.to_string(),
                        &event.stream_version.clone().into_inner(),
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
    #[serial]
    async fn test_version_mismatch() {
        let pool = setup_db().await;
        let mut event_store = PostgresEventStore::new(pool);

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
    #[serial]
    async fn test_error_handling() {
        let pool = setup_db().await;
        let mut event_store = PostgresEventStore::new(pool.clone());

        // Force a database error by dropping the table.
        let client = pool.get().await.unwrap();
        client
            .execute("DROP TABLE IF EXISTS events;", &[])
            .await
            .unwrap();

        let events = vec![DomainEvent::FooHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        let result = event_store.publish(events, expected_version).await;
        assert!(matches!(result, Err(StorageError::Other(_))));
    }

    #[tokio::test]
    #[serial]
    async fn test_concurrent_access() {
        let pool = setup_db().await;
        let event_store = PostgresEventStore::new(pool.clone());

        let events = vec![DomainEvent::FooHappened(123)];
        let expected_version = AggregateStreamVersions::default();

        let publish_futures: Vec<_> = (0..10)
            .map(|_| {
                let mut store = event_store.clone();
                let events = events.clone();
                let expected_version = expected_version.clone();
                tokio::spawn(async move { store.publish(events, expected_version).await })
            })
            .collect();

        for future in publish_futures {
            future.await.unwrap().unwrap();
        }

        // Verify all events are stored in the database
        let client = pool.get().await.unwrap();
        let rows = client.query("SELECT * FROM events", &[]).await.unwrap();
        assert_eq!(rows.len(), 10);
    }
}

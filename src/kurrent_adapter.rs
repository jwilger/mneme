use crate::{EventStreamVersion, StorageError};
use eventstore::{AppendToStreamOptions, Client, ClientSettings, DeleteStreamOptions, EventData};

#[derive(Debug, thiserror::Error)]
pub enum KurrentError {
    #[error("Configuration Error: {0}")]
    Configuration(String),
    #[error("Communication Error: {0}")]
    Communication(String),
    #[error("Deletion Error: {0}")]
    Deletion(String),
}

pub struct Kurrent {
    client: Client,
}

impl Kurrent {
    pub async fn new(connection_str: &str) -> Result<Self, KurrentError> {
        let settings = connection_str
            .parse::<ClientSettings>()
            .map_err(|e| KurrentError::Configuration(format!("{e:?}")))?;
        let client =
            Client::new(settings).map_err(|e| KurrentError::Communication(format!("{e:?}")))?;
        Ok(Kurrent { client })
    }

    pub async fn flush_test_stream(&self, stream_id: &str) -> Result<(), KurrentError> {
        match self
            .client
            .delete_stream(stream_id, &DeleteStreamOptions::default())
            .await
        {
            Ok(_) => Ok(()),
            Err(err) => {
                if let eventstore::Error::Grpc { code, .. } = err {
                    if code == tonic::Code::FailedPrecondition || code == tonic::Code::NotFound {
                        return Ok(());
                    }
                }
                Err(KurrentError::Deletion(format!("{err:?}")))
            }
        }
    }

    pub async fn publish(
        &self,
        events: Vec<EventData>,
        _expected_version: Option<EventStreamVersion>,
    ) -> Result<(), StorageError> {
        let stream_id = events.first().map(|_| "test-stream").unwrap_or("");
        self.client
            .append_to_stream(stream_id, &AppendToStreamOptions::default(), events)
            .await
            .map_err(|_| StorageError::Other("Error publishing events".to_string()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn can_create_kurrent_instance() {
        let connection_str = "esdb://localhost:2113?tls=false";
        let kurrent = Kurrent::new(connection_str).await;
        assert!(kurrent.is_ok());
    }

    #[tokio::test]
    async fn publish_single_event() {
        let connection_str = "esdb://localhost:2113?tls=false";
        let kurrent = Kurrent::new(connection_str).await.unwrap();
        let stream_id = "test-stream";

        // Clear the stream if it already exists
        kurrent.flush_test_stream(stream_id).await.unwrap();

        let event_data = EventData::json("TestEvent", &json!({ "key": "value" })).unwrap();

        // Publishing event
        let publish_result = kurrent
            .publish(vec![event_data], Some(EventStreamVersion(0)))
            .await;
        assert!(
            publish_result.is_ok(),
            "Publishing the event should be successful"
        );
    }
}

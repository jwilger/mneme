//! # Mneme
//!
//! Mneme is an event-sourcing library for Rust that provides a robust foundation for building
//! event-sourced systems. It offers a clean, type-safe API for managing event streams, processing
//! commands, and maintaining aggregate state.
//!
//! ## Core Concepts
//!
//! ### Events
//!
//! Events are immutable facts that represent something that happened in your system. They are the
//! source of truth in an event-sourced system.
//!
//! ```rust
//! use mneme::Event;
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! enum OrderEvent {
//!     Created {
//!         order_id: String,
//!         customer_id: String,
//!         items: Vec<String>,
//!     },
//!     ItemAdded {
//!         item_id: String,
//!     },
//! }
//!
//! impl Event for OrderEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             OrderEvent::Created { .. } => "OrderCreated".to_string(),
//!             OrderEvent::ItemAdded { .. } => "OrderItemAdded".to_string(),
//!         }
//!     }
//! }
//! ```
//!
//! ### Commands
//!
//! Commands represent intentions to change the system state. They are validated and processed to
//! generate events.
//!
//! ```rust
//! use mneme::{Command, Event, AggregateState};
//! use serde::{Serialize, Deserialize};
//!
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! enum UserEvent {
//!     Created { id: String }
//! }
//!
//! impl Event for UserEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             UserEvent::Created { .. } => "UserCreated".to_string()
//!         }
//!     }
//! }
//!
//! #[derive(Debug, Clone)]
//! struct User {
//!     id: Option<String>,
//! }
//!
//! impl AggregateState<UserEvent> for User {
//!     fn apply(&self, event: UserEvent) -> Self {
//!         match event {
//!             UserEvent::Created { id } => User { id: Some(id) }
//!         }
//!     }
//! }
//!
//! #[derive(Clone)]
//! struct CreateUser {
//!     id: String,
//!     state: User,
//! }
//!
//! #[derive(Debug)]
//! struct CommandError(String);
//!
//! impl std::fmt::Display for CommandError {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "{}", self.0)
//!     }
//! }
//!
//! impl std::error::Error for CommandError {}
//!
//! impl Command<UserEvent> for CreateUser {
//!     type State = User;
//!     type Error = CommandError;
//!
//!     fn handle(&self) -> Result<Vec<UserEvent>, Self::Error> {
//!         Ok(vec![UserEvent::Created {
//!             id: self.id.clone()
//!         }])
//!     }
//!
//!     fn event_stream_id(&self) -> mneme::EventStreamId {
//!         mneme::EventStreamId::new()
//!     }
//!
//!     fn get_state(&self) -> Self::State {
//!         self.state.clone()
//!     }
//!
//!     fn set_state(&self, state: Self::State) -> Self {
//!         Self {
//!             state,
//!             ..self.clone()
//!         }
//!     }
//! }
//! ```
//!
//! ### Event Store
//!
//! The event store is responsible for persisting and retrieving events.
//!
//! ```rust,no_run
//! use mneme::{EventStore, ConnectionSettings, Event, EventStream};
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! enum OrderEvent {
//!     Created { id: String }
//! }
//!
//! impl Event for OrderEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             OrderEvent::Created { .. } => "OrderCreated".to_string()
//!         }
//!     }
//! }
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! // Initialize settings from environment variables
//! let settings = ConnectionSettings::from_env()?;
//! let store = EventStore::new(&settings)?;
//!
//! // Read a stream
//! let stream_id = mneme::EventStreamId::new();
//! let mut stream: EventStream<OrderEvent> = store
//!     .stream_builder(stream_id.clone())
//!     .max_count(100)
//!     .read()
//!     .await?;
//!
//! while let Some((event, version)) = stream.next().await? {
//!     println!("Event at version {}: {:?}", version.value(), event);
//! }
//!
//! // Write to a stream
//! let events = vec![OrderEvent::Created {
//!     id: "order-123".to_string()
//! }];
//!
//! store
//!     .stream_writer(stream_id)
//!     .append(events)
//!     .await?;
//! # Ok(())
//! # }
//! ```
//!
//! ### Aggregate State
//!
//! Aggregates represent the current state of your domain objects, built by applying events in sequence.
//!
//! ```rust
//! use mneme::AggregateState;
//! use serde::{Serialize, Deserialize};
//! use mneme::Event;
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! enum AccountEvent {
//!     Created { balance: u64 },
//!     Deposited { amount: u64 },
//! }
//!
//! impl Event for AccountEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             AccountEvent::Created { .. } => "AccountCreated".to_string(),
//!             AccountEvent::Deposited { .. } => "AccountDeposited".to_string(),
//!         }
//!     }
//! }
//!
//! #[derive(Debug, Clone)]
//! struct Account {
//!     balance: u64,
//! }
//!
//! impl AggregateState<AccountEvent> for Account {
//!     fn apply(&self, event: AccountEvent) -> Self {
//!         match event {
//!             AccountEvent::Created { balance } => Account { balance },
//!             AccountEvent::Deposited { amount } => Account {
//!                 balance: self.balance + amount,
//!             },
//!         }
//!     }
//! }
//! ```
//!
//! ## Example: Bank Account
//!
//! Here's a complete example showing how all the pieces fit together:
//!
//! ```rust,no_run
//! use mneme::{Command, Event, AggregateState, EventStreamId, EventStore, ConnectionSettings};
//! use serde::{Serialize, Deserialize};
//!
//! // Events
//! #[derive(Debug, Serialize, Deserialize)]
//! enum BankAccountEvent {
//!     Created { id: String, balance: u64 },
//!     Deposited { amount: u64 },
//! }
//!
//! impl Event for BankAccountEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             BankAccountEvent::Created { .. } => "BankAccountCreated".to_string(),
//!             BankAccountEvent::Deposited { .. } => "AmountDeposited".to_string(),
//!         }
//!     }
//! }
//!
//! // State
//! #[derive(Debug, Clone)]
//! struct BankAccount {
//!     id: Option<String>,
//!     balance: u64,
//! }
//!
//! impl Default for BankAccount {
//!     fn default() -> Self {
//!         Self {
//!             id: None,
//!             balance: 0,
//!         }
//!     }
//! }
//!
//! impl AggregateState<BankAccountEvent> for BankAccount {
//!     fn apply(&self, event: BankAccountEvent) -> Self {
//!         match event {
//!             BankAccountEvent::Created { id, balance } => BankAccount {
//!                 id: Some(id),
//!                 balance,
//!             },
//!             BankAccountEvent::Deposited { amount } => BankAccount {
//!                 balance: self.balance + amount,
//!                 ..self.clone()
//!             },
//!         }
//!     }
//! }
//!
//! // Commands
//! #[derive(Clone)]
//! struct CreateAccount {
//!     id: String,
//!     initial_balance: u64,
//!     state: BankAccount,
//! }
//!
//! #[derive(Debug)]
//! struct CommandError(String);
//!
//! impl std::fmt::Display for CommandError {
//!     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//!         write!(f, "{}", self.0)
//!     }
//! }
//!
//! impl std::error::Error for CommandError {}
//!
//! impl Command<BankAccountEvent> for CreateAccount {
//!     type State = BankAccount;
//!     type Error = CommandError;
//!
//!     fn handle(&self) -> Result<Vec<BankAccountEvent>, Self::Error> {
//!         Ok(vec![BankAccountEvent::Created {
//!             id: self.id.clone(),
//!             balance: self.initial_balance,
//!         }])
//!     }
//!
//!     fn event_stream_id(&self) -> EventStreamId {
//!         EventStreamId::new()
//!     }
//!
//!     fn get_state(&self) -> Self::State {
//!         self.state.clone()
//!     }
//!
//!     fn set_state(&self, state: Self::State) -> Self {
//!         Self {
//!             state,
//!             ..self.clone()
//!         }
//!     }
//! }
//!
//! # async fn example() -> Result<(), mneme::Error> {
//! // Usage
//! let settings = ConnectionSettings::from_env()?;
//! let store = EventStore::new(&settings)?;
//!
//! let command = CreateAccount {
//!     id: "acc123".to_string(),
//!     initial_balance: 1000,
//!     state: BankAccount::default(),
//! };
//!
//! let stream_id = command.event_stream_id();
//! let events = command.handle().map_err(|e| mneme::Error::CommandFailed {
//!     message: e.to_string(),
//!     attempt: 1,
//!     max_attempts: 1,
//!     source: Box::new(e),
//! })?;
//!
//! store.stream_writer(stream_id)
//!     .append(events).await?;
//! # Ok(())
//! # }
//! ```

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
pub use event_store::EventStore;
pub use kurrent_adapter::{
    ConnectionSettings, EventStream, EventStreamId, EventStreamVersion, Kurrent,
};

use delay::RetryDelay;

pub async fn execute<E, C, S>(
    command: C,
    event_store: &mut S,
    config: ExecuteConfig,
) -> Result<(), Error>
where
    E: Event,
    C: Command<E> + Clone + Send,
    S: EventStore + Send,
{
    // Create metrics for this execution
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

        // Read and apply existing events from the stream to rebuild the aggregate state
        let read_result = event_store.read_stream(command.event_stream_id()).await;

        match read_result {
            // Stream doesn't exist yet, which is fine for a new aggregate
            Err(Error::EventStoreOther(eventstore::Error::ResourceNotFound)) => {}

            // Other errors should be propagated
            Err(other) => {
                break Err(other);
            }

            // Stream exists, so apply all events to rebuild the aggregate state
            Ok(mut event_stream) => {
                while let Some((event, version)) = event_stream.next().await? {
                    command = command.apply(event);
                    expected_version = Some(version);
                }
            }
        }

        // Now handle the command and produce new events
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

        // Only publish if there are events to publish
        if !domain_events.is_empty() {
            // Let the command override the expected version if it wants to
            let append_options = match (command.override_expected_version(), expected_version) {
                (Some(v), _) => eventstore::AppendToStreamOptions::default()
                    .expected_revision(eventstore::ExpectedRevision::Exact(v)),
                (None, Some(v)) => eventstore::AppendToStreamOptions::default()
                    .expected_revision(eventstore::ExpectedRevision::Exact(v.value())),
                (None, None) => Default::default(),
            };

            match event_store
                .publish(command.event_stream_id(), domain_events, &append_options)
                .await
            {
                Ok(_) => {
                    break Ok(());
                }
                Err(Error::EventStoreVersionMismatch { .. }) => {
                    // Calculate delay with exponential backoff and jitter
                    let delay = config.retry_delay().calculate_delay(retries);
                    tokio::time::sleep(delay).await;

                    // Mark command as being retried and increment retry counter
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

    // Stop the timer before returning

    result
}

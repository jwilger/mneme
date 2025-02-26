//! Command handling for the event sourcing system.
//!
//! This module provides traits for implementing commands and aggregate states in an
//! event-sourced system. Commands represent intentions to modify the system state,
//! while aggregate states represent the current state of domain objects.
//!
//! # Examples
//!
//! Here's a complete example of implementing a command and its associated state:
//!
//! ```rust
//! use mneme::{Command, Event, AggregateState, EventStreamId};
//! use serde::{Serialize, Deserialize};
//!
//! // Define your events
//! #[derive(Debug, Serialize, Deserialize)]
//! enum BankAccountEvent {
//!     Created { id: String, balance: u64 }
//! }
//!
//! impl Event for BankAccountEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             BankAccountEvent::Created { .. } => "BankAccountCreated".to_string()
//!         }
//!     }
//! }
//!
//! // Define your aggregate state
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
//!             }
//!         }
//!     }
//! }
//!
//! // Define your command
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
//!     fn empty_state(&self) -> Self::State {
//!         BankAccount::default()
//!     }
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
//! ```

use crate::event::Event;
use crate::store::EventStreamId;
use std::fmt::Debug;

/// Represents a command that can be executed to produce events.
///
/// Commands are the entry point for modifying system state in an event-sourced system.
/// Each command implementation defines how to handle the command and produce events,
/// as well as how to manage the associated aggregate state.
///
/// # Type Parameters
///
/// * `E` - The event type this command produces
///
/// # Examples
///
/// ```rust
/// use mneme::{Command, Event, AggregateState, EventStreamId};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Serialize, Deserialize)]
/// enum BankAccountEvent {
///     Deposited { amount: u64 }
/// }
///
/// impl Event for BankAccountEvent {
///     fn event_type(&self) -> String {
///         match self {
///             BankAccountEvent::Deposited { .. } => "AmountDeposited".to_string()
///         }
///     }
/// }
///
/// #[derive(Debug, Default, Clone)]
/// struct BankAccount {
///     balance: u64,
/// }
///
/// impl AggregateState<BankAccountEvent> for BankAccount {
///     fn apply(&self, event: BankAccountEvent) -> Self {
///         match event {
///             BankAccountEvent::Deposited { amount } => BankAccount {
///                 balance: self.balance + amount,
///             }
///         }
///     }
/// }
///
/// #[derive(Clone)]
/// struct Deposit {
///     amount: u64,
///     state: BankAccount,
/// }
///
/// #[derive(Debug)]
/// struct CommandError(String);
///
/// impl std::fmt::Display for CommandError {
///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
///         write!(f, "{}", self.0)
///     }
/// }
///
/// impl std::error::Error for CommandError {}
///
/// impl Command<BankAccountEvent> for Deposit {
///     type State = BankAccount;
///     type Error = CommandError;
///
///     fn empty_state(&self) -> Self::State {
///         BankAccount::default()
///     }
///
///     fn handle(&self) -> Result<Vec<BankAccountEvent>, Self::Error> {
///         Ok(vec![BankAccountEvent::Deposited {
///             amount: self.amount,
///         }])
///     }
///
///     fn event_stream_id(&self) -> EventStreamId {
///         EventStreamId::new()
///     }
///
///     fn get_state(&self) -> Self::State {
///         self.state.clone()
///     }
///
///     fn set_state(&self, state: Self::State) -> Self {
///         Self {
///             state,
///             ..self.clone()
///         }
///     }
/// }
/// ```
pub trait Command<E: Event> {
    /// The aggregate state type for this command
    type State: AggregateState<E>;
    
    /// The error type that can be returned when handling this command
    type Error: std::error::Error + Send + Sync + 'static;

    /// Creates an empty state for the aggregate
    fn empty_state(&self) -> Self::State;
    
    /// Handles the command and produces events
    ///
    /// This is the main entry point for command execution. It should validate
    /// the command against the current state and return the events that should
    /// be applied.
    fn handle(&self) -> Result<Vec<E>, Self::Error>;
    
    /// Returns the stream ID for this command's events
    fn event_stream_id(&self) -> EventStreamId;
    
    /// Gets the current aggregate state
    fn get_state(&self) -> Self::State;
    
    /// Sets a new aggregate state
    fn set_state(&self, state: Self::State) -> Self;
    
    /// Called when a command is being retried
    ///
    /// The default implementation simply clones the command. Override this
    /// if special handling is needed for retries.
    fn mark_retry(&self) -> Self where Self: Sized + Clone {
        self.clone()
    }
    
    /// Overrides the expected version for optimistic concurrency control
    ///
    /// Return None to use the default version checking behavior.
    fn override_expected_version(&self) -> Option<u64> {
        None
    }
    
    /// Applies an event to the command's state
    ///
    /// The default implementation updates the state using the AggregateState trait.
    fn apply(&mut self, event: E) -> Self where Self: Sized {
        self.set_state(self.get_state().apply(event))
    }
}

/// Unit type implementation of Command for testing
impl<E: Event> Command<E> for () {
    type State = ();
    type Error = std::convert::Infallible;

    fn empty_state(&self) -> Self::State {}
    fn handle(&self) -> Result<Vec<E>, Self::Error> { Ok(vec![]) }
    fn event_stream_id(&self) -> EventStreamId { EventStreamId::new() }
    fn get_state(&self) -> Self::State {}
    fn set_state(&self, _: Self::State) -> Self {}
    fn mark_retry(&self) -> Self {}
}

/// Represents the state of an aggregate that can be modified by events
///
/// # Type Parameters
///
/// * `E` - The event type that can modify this state
///
/// # Examples
///
/// ```rust
/// use mneme::{AggregateState, Event};
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Serialize, Deserialize)]
/// enum CustomerEvent {
///     Created { name: String },
///     NameChanged { name: String },
/// }
///
/// impl Event for CustomerEvent {
///     fn event_type(&self) -> String {
///         match self {
///             CustomerEvent::Created { .. } => "CustomerCreated".to_string(),
///             CustomerEvent::NameChanged { .. } => "CustomerNameChanged".to_string(),
///         }
///     }
/// }
///
/// #[derive(Debug, Default, Clone)]
/// struct Customer {
///     name: String,
/// }
///
/// impl AggregateState<CustomerEvent> for Customer {
///     fn apply(&self, event: CustomerEvent) -> Self {
///         match event {
///             CustomerEvent::Created { name } => Customer { name },
///             CustomerEvent::NameChanged { name } => Customer { name },
///         }
///     }
/// }
/// ```
pub trait AggregateState<E: Event>: Debug + Sized {
    /// Apply an event to the current state and return the new state
    fn apply(&self, event: E) -> Self;
}

/// Unit type implementation of AggregateState for testing
impl<E: Event> AggregateState<E> for () {
    fn apply(&self, _: E) {}
}
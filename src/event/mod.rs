//! Event handling for the event sourcing system.
//!
//! This module defines the core Event trait that all domain events must implement.
//! Events represent facts about what has happened in the system and are the source
//! of truth for the system's state.
//!
//! # Examples
//!
//! Here's how to define and use events in your domain:
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
//!     Cancelled {
//!         reason: String,
//!     },
//! }
//!  
//! impl Event for OrderEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             OrderEvent::Created { .. } => "OrderCreated".to_string(),
//!             OrderEvent::ItemAdded { .. } => "OrderItemAdded".to_string(),
//!             OrderEvent::Cancelled { .. } => "OrderCancelled".to_string(),
//!         }
//!     }
//! }
//!
//! // Using events with serde_json
//! let event = OrderEvent::Created {
//!     order_id: "order123".to_string(),
//!     customer_id: "cust456".to_string(),
//!     items: vec!["item1".to_string(), "item2".to_string()],
//! };
//!
//! let json = serde_json::to_string(&event).unwrap();
//! let deserialized: OrderEvent = serde_json::from_str(&json).unwrap();
//! ```
//!
//! # Best Practices
//!
//! When designing events:
//!
//! 1. Make them immutable facts about what has happened
//! 2. Include all necessary data in the event itself
//! 3. Use meaningful names that reflect past tense actions
//! 4. Consider versioning strategy for schema evolution
//! 5. Keep events focused and atomic
//!
//! # Schema Evolution
//!
//! When your events need to evolve over time, consider these strategies:
//!
//! ```rust
//! use mneme::Event;
//! use serde::{Serialize, Deserialize};
//!
//! #[derive(Debug, Serialize, Deserialize)]
//! #[serde(tag = "version")]
//! enum UserEvent {
//!     #[serde(rename = "v1")]
//!     V1 {
//!         user_id: String,
//!         name: String,
//!     },
//!     #[serde(rename = "v2")]
//!     V2 {
//!         user_id: String,
//!         first_name: String,
//!         last_name: String,
//!     },
//! }
//!
//! impl Event for UserEvent {
//!     fn event_type(&self) -> String {
//!         match self {
//!             UserEvent::V1 { .. } => "UserCreated_v1".to_string(),
//!             UserEvent::V2 { .. } => "UserCreated_v2".to_string(),
//!         }
//!     }
//! }
//! ```

use std::fmt::Debug;
use serde::{Deserialize, Serialize};

/// Represents a domain event in the event sourcing system.
///
/// Events are immutable facts about what has happened in your system. They must be
/// serializable and deserializable to support persistence, and they must provide
/// a way to identify their type.
///
/// # Type Parameters
///
/// The trait is automatically implemented for types that implement the necessary
/// bounds:
/// * `Debug` - For debugging and logging
/// * `Deserialize` - For reconstructing events from storage
/// * `Serialize` - For storing events
/// * `Send` - For thread safety
/// * `Sync` - For thread safety
/// * `Sized` - For compile-time size determination
///
/// # Examples
///
/// Basic event implementation:
///
/// ```rust
/// use mneme::Event;
/// use serde::{Serialize, Deserialize};
///
/// #[derive(Debug, Serialize, Deserialize)]
/// enum PaymentEvent {
///     Received { amount: u64, currency: String },
///     Refunded { amount: u64, reason: String },
/// }
///
/// impl Event for PaymentEvent {
///     fn event_type(&self) -> String {
///         match self {
///             PaymentEvent::Received { .. } => "PaymentReceived".to_string(),
///             PaymentEvent::Refunded { .. } => "PaymentRefunded".to_string(),
///         }
///     }
/// }
/// ```
///
/// Using with custom types:
///
/// ```rust
/// use mneme::Event;
/// use serde::{Serialize, Deserialize};
/// use std::fmt;
///
/// #[derive(Serialize, Deserialize)]
/// struct Money {
///     amount: u64,
///     currency: String,
/// }
///
/// impl fmt::Debug for Money {
///     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
///         write!(f, "{} {}", self.amount, self.currency)
///     }
/// }
///
/// #[derive(Debug, Serialize, Deserialize)]
/// enum TransactionEvent {
///     Initiated { money: Money },
///     Completed { money: Money, timestamp: String },
/// }
///
/// impl Event for TransactionEvent {
///     fn event_type(&self) -> String {
///         match self {
///             TransactionEvent::Initiated { .. } => "TransactionInitiated".to_string(),
///             TransactionEvent::Completed { .. } => "TransactionCompleted".to_string(),
///         }
///     }
/// }
/// ```
pub trait Event: Debug + for<'de> Deserialize<'de> + Serialize + Send + Sync + Sized {
    /// Returns a string identifier for the event type.
    ///
    /// This identifier is used when storing and retrieving events, and can be
    /// useful for versioning and debugging.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use mneme::Event;
    /// use serde::{Serialize, Deserialize};
    ///
    /// #[derive(Debug, Serialize, Deserialize)]
    /// enum UserEvent {
    ///     Created { id: String },
    ///     Updated { id: String, name: String },
    /// }
    ///
    /// impl Event for UserEvent {
    ///     fn event_type(&self) -> String {
    ///         match self {
    ///             UserEvent::Created { .. } => "UserCreated".to_string(),
    ///             UserEvent::Updated { .. } => "UserUpdated".to_string(),
    ///         }
    ///     }
    /// }
    /// ```
    fn event_type(&self) -> String;
}

/// Unit type implementation of Event for testing
impl Event for () {
    fn event_type(&self) -> String {
        "None".to_string()
    }
}
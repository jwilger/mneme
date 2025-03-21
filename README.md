# Mneme

[![Crates.io](https://img.shields.io/crates/v/mneme)](https://crates.io/crates/mneme)
[![Documentation](https://docs.rs/mneme/badge.svg)](https://docs.rs/mneme)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

Mneme is a robust event-sourcing library for Rust projects. Named after the
Greek muse of memory, Mneme helps you maintain a complete and accurate history
of your domain events. It is built on top of [Kurrent](https://kurrent.dev/)
(formerly EventStoreDB) and provides a clean, type-safe API for implementing
event-sourced systems.

## Getting Started

### Installation

Add Mneme to your `Cargo.toml`:

```toml
[dependencies]
mneme = "0.1.0"
```

### Basic Concepts

Mneme implements the event sourcing pattern with these core components:

- **Commands**: Operations that may produce events
- **Events**: Facts that have happened in the system
- **State**: Derived from applying events sequentially
- **Event Store**: Persists the event stream for each aggregate

### Usage Example

```rust
use mneme::{AggregateState, Command, Event, EventStore, EventStreamId, execute};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// 1. Define your events
#[derive(Debug, Clone, Deserialize, Serialize)]
enum BankAccountEvent {
    Created { id: Uuid, owner: String },
    Deposited { id: Uuid, amount: u32 },
    Withdrawn { id: Uuid, amount: u32 }
}

impl Event for BankAccountEvent {
    fn event_type(&self) -> String {
        match self {
            BankAccountEvent::Created { .. } => "BankAccount.Created".to_string(),
            BankAccountEvent::Deposited { .. } => "BankAccount.Deposited".to_string(),
            BankAccountEvent::Withdrawn { .. } => "BankAccount.Withdrawn".to_string(),
        }
    }
}

// 2. Define your aggregate state
#[derive(Clone, Debug)]
struct AccountState {
    balance: u32,
}

impl AggregateState<BankAccountEvent> for AccountState {
    fn apply(&mut self, event: &BankAccountEvent) -> &Self {
        match event {
            BankAccountEvent::Created { .. } => {},
            BankAccountEvent::Deposited { amount, .. } => {
                self.balance += amount;
            },
            BankAccountEvent::Withdrawn { amount, .. } => {
                self.balance -= amount;
            }
        }
        self
    }
}

// 3. Define a command
#[derive(Clone)]
struct WithdrawCommand {
    id: Uuid,
    amount: u32,
    state: AccountState,
}

impl Command for WithdrawCommand {
    type Event = BankAccountEvent;
    type State = AccountState;
    type Error = String;

    fn get_state(&self) -> Self::State {
        self.state.clone()
    }

    fn set_state(&mut self, state: &Self::State) {
        self.state = state.clone();
    }

    fn event_stream_id(&self) -> EventStreamId {
        EventStreamId(self.id)
    }

    fn handle(&self) -> Result<Vec<Self::Event>, Self::Error> {
        if self.amount <= self.state.balance {
            Ok(vec![BankAccountEvent::Withdrawn { 
                id: self.id, 
                amount: self.amount 
            }])
        } else {
            Err("Insufficient funds".to_string())
        }
    }
}

// 4. Use the execute function with your event store
async fn process_withdrawal(account_id: Uuid, amount: u32) -> Result<(), mneme::Error> {
    let mut event_store = /* your event store implementation */;
    
    let command = WithdrawCommand {
        id: account_id,
        amount,
        state: AccountState { balance: 0 }, // Initial state will be replaced by stored events
    };
    
    execute(command, &mut event_store, Default::default()).await
}
```

## Advanced Features

- **Optimistic Concurrency**: Handles concurrent updates to the same event stream
- **State Reconstruction**: Automatically rebuilds aggregate state from event history
- **Type Safety**: Leverages Rust's type system for safe event handling

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE)
file for details.

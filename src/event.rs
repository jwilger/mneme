use serde::{Deserialize, Serialize};
use std::fmt::Debug;

pub trait Event: Debug + for<'de> Deserialize<'de> + Serialize + Send + Sync + Sized {
    fn event_type(&self) -> String;
}

impl Event for () {
    fn event_type(&self) -> String {
        "None".to_string()
    }
}


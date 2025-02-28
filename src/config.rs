use crate::delay::RetryDelay;
use crate::error::Error;

const MAX_RETRIES_LIMIT: u32 = 10;
const MIN_DELAY_MS: u64 = 50;
const MAX_DELAY_MS: u64 = 5000;

#[derive(Debug, Clone)]
pub struct ExecuteConfig {
    max_retries: u32,
    retry_delay: RetryDelay,
}

impl ExecuteConfig {
    pub fn with_max_retries(mut self, max_retries: u32) -> Result<Self, Error> {
        if max_retries == 0 {
            return Err(Error::InvalidConfig {
                message: "max_retries cannot be 0".to_string(),
                parameter: Some("max_retries".to_string()),
            });
        }
        if max_retries > MAX_RETRIES_LIMIT {
            return Err(Error::InvalidConfig {
                message: format!("max_retries cannot exceed {MAX_RETRIES_LIMIT}"),
                parameter: Some("max_retries".to_string()),
            });
        }
        self.max_retries = max_retries;
        Ok(self)
    }

    pub fn with_base_delay(mut self, delay_ms: u64) -> Result<Self, Error> {
        if delay_ms == 0 {
            return Err(Error::InvalidConfig {
                message: "base_retry_delay_ms cannot be 0".to_string(),
                parameter: Some("base_retry_delay_ms".to_string()),
            });
        }
        if delay_ms < MIN_DELAY_MS {
            return Err(Error::InvalidConfig {
                message: format!("base_retry_delay_ms must be at least {MIN_DELAY_MS}ms"),
                parameter: Some("base_retry_delay_ms".to_string()),
            });
        }
        if delay_ms > MAX_DELAY_MS {
            return Err(Error::InvalidConfig {
                message: format!("base_retry_delay_ms cannot exceed {MAX_DELAY_MS}ms"),
                parameter: Some("base_retry_delay_ms".to_string()),
            });
        }
        // Update retry delay config with new base delay but keep max delay
        self.retry_delay = RetryDelay::new(delay_ms, self.retry_delay.max_delay_ms());
        Ok(self)
    }

    pub fn with_max_delay(mut self, max_delay_ms: u64) -> Result<Self, Error> {
        if max_delay_ms < self.retry_delay.base_delay_ms() {
            return Err(Error::InvalidConfig {
                message: format!(
                    "max_delay_ms ({max_delay_ms}) cannot be less than base_delay_ms ({})",
                    self.retry_delay.base_delay_ms()
                ),
                parameter: Some("max_delay_ms".to_string()),
            });
        }
        self.retry_delay = RetryDelay::new(self.retry_delay.base_delay_ms(), max_delay_ms);
        Ok(self)
    }

    pub fn max_retries(&self) -> u32 {
        self.max_retries
    }

    pub fn retry_delay(&self) -> &RetryDelay {
        &self.retry_delay
    }
}

impl Default for ExecuteConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            retry_delay: RetryDelay::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_max_retries() {
        match ExecuteConfig::default().with_max_retries(0) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(message, "max_retries cannot be 0");
                assert_eq!(parameter, Some("max_retries".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        match ExecuteConfig::default().with_max_retries(MAX_RETRIES_LIMIT + 1) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(
                    message,
                    format!("max_retries cannot exceed {MAX_RETRIES_LIMIT}")
                );
                assert_eq!(parameter, Some("max_retries".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        let config = ExecuteConfig::default()
            .with_max_retries(5)
            .expect("Failed to set valid max_retries");
        assert_eq!(config.max_retries(), 5);
    }

    #[test]
    fn validates_base_delay() {
        match ExecuteConfig::default().with_base_delay(0) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(message, "base_retry_delay_ms cannot be 0");
                assert_eq!(parameter, Some("base_retry_delay_ms".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        match ExecuteConfig::default().with_base_delay(MIN_DELAY_MS - 1) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(
                    message,
                    format!("base_retry_delay_ms must be at least {MIN_DELAY_MS}ms")
                );
                assert_eq!(parameter, Some("base_retry_delay_ms".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        match ExecuteConfig::default().with_base_delay(MAX_DELAY_MS + 1) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(
                    message,
                    format!("base_retry_delay_ms cannot exceed {MAX_DELAY_MS}ms")
                );
                assert_eq!(parameter, Some("base_retry_delay_ms".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        let config = ExecuteConfig::default()
            .with_base_delay(200)
            .expect("Failed to set valid base_delay");
        assert_eq!(config.retry_delay().base_delay_ms(), 200);
    }

    #[test]
    fn validates_max_delay() {
        let config = ExecuteConfig::default().with_base_delay(100).unwrap();

        match config.clone().with_max_delay(50) {
            Err(Error::InvalidConfig {
                message, parameter, ..
            }) => {
                assert_eq!(
                    message,
                    "max_delay_ms (50) cannot be less than base_delay_ms (100)"
                );
                assert_eq!(parameter, Some("max_delay_ms".to_string()));
            }
            other => panic!("Expected InvalidConfig error, got {:?}", other),
        }

        let config = config
            .with_max_delay(1000)
            .expect("Failed to set valid max_delay");
        assert_eq!(config.retry_delay().max_delay_ms(), 1000);
    }

    #[test]
    fn default_values_are_valid() {
        let config = ExecuteConfig::default();
        assert!(
            config
                .clone()
                .with_max_retries(config.max_retries())
                .is_ok()
        );
        assert!(
            config
                .clone()
                .with_base_delay(config.retry_delay().base_delay_ms())
                .is_ok()
        );
    }
}

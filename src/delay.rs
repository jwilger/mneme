use rand::prelude::*;
use std::cell::RefCell;
use tokio::time::Duration;

thread_local! {
    static THREAD_RNG: RefCell<SmallRng> = RefCell::new(SmallRng::seed_from_u64(0));
}

/// Configuration for retry delays with jitter
#[derive(Debug, Clone, Copy)]
pub struct RetryDelay {
    /// Base delay (in milliseconds) before applying exponential backoff and jitter
    base_delay_ms: u64,
    /// Maximum delay (in milliseconds) after applying exponential backoff and jitter
    max_delay_ms: u64,
}

impl RetryDelay {
    /// Creates a new RetryDelay configuration
    pub fn new(base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            base_delay_ms,
            max_delay_ms,
        }
    }

    /// Returns the configured base delay in milliseconds
    pub fn base_delay_ms(&self) -> u64 {
        self.base_delay_ms
    }

    /// Returns the configured maximum delay in milliseconds
    pub fn max_delay_ms(&self) -> u64 {
        self.max_delay_ms
    }

    /// Calculates the delay for a given retry attempt using exponential backoff with full jitter.
    ///
    /// The algorithm:
    /// 1. Calculates exponential delay: base_delay * 2^retry_count
    /// 2. Caps at max_delay
    /// 3. Applies full jitter by randomly selecting a value between 0 and the calculated delay
    ///
    /// This helps prevent the "thundering herd" problem in distributed systems by
    /// ensuring retrying clients don't all hit the server at the same time.
    pub fn calculate_delay(&self, retry_count: u32) -> Duration {
        // Calculate exponential delay
        let exp_delay = self.base_delay_ms * 2u64.pow(retry_count);

        // Cap at max delay
        let capped_delay = exp_delay.min(self.max_delay_ms);

        // Apply full jitter using thread-local RNG
        let jittered_delay = THREAD_RNG.with(|rng| {
            #[allow(deprecated)]
            rng.borrow_mut().gen_range(0..=capped_delay)
        });

        Duration::from_millis(jittered_delay)
    }
}

impl Default for RetryDelay {
    fn default() -> Self {
        Self {
            base_delay_ms: 100,
            max_delay_ms: 30_000, // 30 seconds max delay
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn calculates_delay_within_bounds() {
        let retry_delay = RetryDelay::new(100, 1000);

        // Test multiple times to account for randomness
        for _ in 0..100 {
            let delay = retry_delay.calculate_delay(0);
            assert!(
                delay.as_millis() <= 100,
                "First retry delay should be <= base delay"
            );

            let delay = retry_delay.calculate_delay(1);
            assert!(
                delay.as_millis() <= 200,
                "Second retry delay should be <= 2 * base delay"
            );

            let delay = retry_delay.calculate_delay(3);
            assert!(
                delay.as_millis() <= 800,
                "Fourth retry delay should be <= 8 * base delay"
            );

            // Test max delay cap
            let delay = retry_delay.calculate_delay(5);
            assert!(
                delay.as_millis() <= 1000,
                "Delay should be capped at max_delay"
            );
        }
    }

    #[test]
    fn applies_jitter() {
        let retry_delay = RetryDelay::new(100, 1000);
        let mut delays = Vec::new();

        // Collect multiple delays to check for variation
        for _ in 0..100 {
            let delay = retry_delay.calculate_delay(1);
            delays.push(delay.as_millis());
        }

        // Check that we get some variation in the delays
        let unique_delays = delays.iter().collect::<HashSet<_>>();
        assert!(
            unique_delays.len() > 1,
            "Jitter should produce varying delays"
        );

        // Check that all delays are within expected bounds
        assert!(
            delays.iter().all(|&d| d <= 200),
            "All delays should be <= 2 * base delay"
        );
    }

    #[test]
    fn respects_max_delay() {
        let retry_delay = RetryDelay::new(100, 500);

        // Even with a high retry count, delay should be capped
        for _ in 0..100 {
            let delay = retry_delay.calculate_delay(10); // Would be 102400ms without cap
            assert!(
                delay.as_millis() <= 500,
                "Delay should respect max_delay cap"
            );
        }
    }
}


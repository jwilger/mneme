use rand::prelude::*;
use std::cell::RefCell;
use tokio::time::Duration;

thread_local! {
    static THREAD_RNG: RefCell<SmallRng> = RefCell::new(SmallRng::seed_from_u64(0));
}

#[derive(Debug, Clone, Copy)]
pub struct RetryDelay {
    base_delay_ms: u64,
    max_delay_ms: u64,
}

impl RetryDelay {
    pub fn new(base_delay_ms: u64, max_delay_ms: u64) -> Self {
        Self {
            base_delay_ms,
            max_delay_ms,
        }
    }

    pub fn base_delay_ms(&self) -> u64 {
        self.base_delay_ms
    }

    pub fn max_delay_ms(&self) -> u64 {
        self.max_delay_ms
    }

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

        for _ in 0..100 {
            let delay = retry_delay.calculate_delay(1);
            delays.push(delay.as_millis());
        }

        let unique_delays = delays.iter().collect::<HashSet<_>>();
        assert!(
            unique_delays.len() > 1,
            "Jitter should produce varying delays"
        );

        assert!(
            delays.iter().all(|&d| d <= 200),
            "All delays should be <= 2 * base delay"
        );
    }

    #[test]
    fn respects_max_delay() {
        let retry_delay = RetryDelay::new(100, 500);

        for _ in 0..100 {
            let delay = retry_delay.calculate_delay(10); // Would be 102400ms without cap
            assert!(
                delay.as_millis() <= 500,
                "Delay should respect max_delay cap"
            );
        }
    }
}

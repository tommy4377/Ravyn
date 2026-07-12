use std::{
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::Mutex;

/// Token bucket used for per-job and global bandwidth limits.
pub struct RateLimiter {
    bytes_per_second: AtomicU64,
    state: Mutex<State>,
}

struct State {
    available: f64,
    updated_at: Instant,
}

impl RateLimiter {
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            bytes_per_second: AtomicU64::new(bytes_per_second),
            state: Mutex::new(State {
                available: bytes_per_second as f64,
                updated_at: Instant::now(),
            }),
        }
    }

    pub fn set_bytes_per_second(&self, bytes_per_second: u64) {
        self.bytes_per_second
            .store(bytes_per_second, Ordering::Release);
    }

    pub fn bytes_per_second(&self) -> u64 {
        self.bytes_per_second.load(Ordering::Acquire)
    }

    pub async fn consume(&self, bytes: usize) {
        let configured = self.bytes_per_second();
        if configured == 0 || bytes == 0 {
            return;
        }
        let capacity = configured as f64;
        let mut remaining = bytes as f64;
        while remaining > 0.0 {
            let wait = {
                let mut state = self.state.lock().await;
                let now = Instant::now();
                let elapsed = now.duration_since(state.updated_at).as_secs_f64();
                state.updated_at = now;
                state.available = (state.available + elapsed * capacity).min(capacity);
                let charge = remaining.min(capacity);
                if state.available >= charge {
                    state.available -= charge;
                    remaining -= charge;
                    None
                } else {
                    Some(Duration::from_secs_f64(
                        (charge - state.available) / capacity,
                    ))
                }
            };
            if let Some(duration) = wait {
                tokio::time::sleep(duration).await;
            }
        }
    }
}

#[derive(Clone)]
pub struct RateLimiters {
    job: Arc<RateLimiter>,
    global: Arc<RateLimiter>,
}

impl RateLimiters {
    pub fn new(job: Arc<RateLimiter>, global: Arc<RateLimiter>) -> Self {
        Self { job, global }
    }
    pub async fn consume(&self, bytes: usize) {
        tokio::join!(self.job.consume(bytes), self.global.consume(bytes));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn accepts_chunks_larger_than_bucket_capacity() {
        let limiter = RateLimiter::new(1_000_000);
        tokio::time::timeout(Duration::from_secs(3), limiter.consume(1_500_000))
            .await
            .unwrap();
    }
}

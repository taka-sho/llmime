use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

pub struct LatencyTracker {
    samples: Mutex<VecDeque<u64>>,
    capacity: usize,
}

impl LatencyTracker {
    pub fn new(capacity: usize) -> Arc<Self> {
        Arc::new(Self {
            samples: Mutex::new(VecDeque::with_capacity(capacity)),
            capacity,
        })
    }

    pub fn record(&self, elapsed_ms: u64) {
        let mut samples = self.samples.lock().unwrap();
        if samples.len() == self.capacity {
            samples.pop_front();
        }
        samples.push_back(elapsed_ms);
    }

    pub fn p50(&self) -> Option<u64> {
        self.percentile(50)
    }

    pub fn p95(&self) -> Option<u64> {
        self.percentile(95)
    }

    fn percentile(&self, pct: usize) -> Option<u64> {
        let samples = self.samples.lock().unwrap();
        if samples.is_empty() {
            return None;
        }
        let mut sorted: Vec<u64> = samples.iter().copied().collect();
        sorted.sort_unstable();
        let idx = (sorted.len().saturating_mul(pct) / 100).min(sorted.len() - 1);
        Some(sorted[idx])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_record_and_p50() {
        let tracker = LatencyTracker::new(100);
        assert_eq!(tracker.p50(), None);
        tracker.record(100);
        tracker.record(200);
        tracker.record(300);
        tracker.record(400);
        let p50 = tracker.p50().unwrap();
        assert!((200..=300).contains(&p50), "p50={p50}");
    }

    #[test]
    fn test_p95() {
        let tracker = LatencyTracker::new(100);
        for i in 1..=100u64 {
            tracker.record(i * 10);
        }
        let p95 = tracker.p95().unwrap();
        assert!((940..=1000).contains(&p95), "p95={p95}");
    }

    #[test]
    fn test_capacity_limit() {
        let tracker = LatencyTracker::new(3);
        tracker.record(1000);
        tracker.record(2000);
        tracker.record(3000);
        tracker.record(10); // evicts 1000
        let p50 = tracker.p50().unwrap();
        // samples: [2000, 3000, 10] sorted: [10, 2000, 3000]
        assert_eq!(p50, 2000);
        let count = tracker.samples.lock().unwrap().len();
        assert_eq!(count, 3);
    }

    #[test]
    fn test_single_sample() {
        let tracker = LatencyTracker::new(10);
        tracker.record(500);
        assert_eq!(tracker.p50(), Some(500));
        assert_eq!(tracker.p95(), Some(500));
    }
}

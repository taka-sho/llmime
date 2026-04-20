use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

const COST_QWEN3_30B: f64 = 0.0004202;
const COST_LLAMA_8B_FAST: f64 = 0.000017;

fn cost_per_request(model_id: &str) -> f64 {
    if model_id.contains("qwen3-30b") {
        COST_QWEN3_30B
    } else if model_id.contains("llama") && model_id.contains("8b") {
        COST_LLAMA_8B_FAST
    } else {
        COST_QWEN3_30B
    }
}

pub struct CostMonitor {
    requests_today: AtomicU64,
    requests_hour: AtomicU64,
    cost_per_req_today: AtomicU64,
    cost_per_req_hour: AtomicU64,
}

impl CostMonitor {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            requests_today: AtomicU64::new(0),
            requests_hour: AtomicU64::new(0),
            cost_per_req_today: AtomicU64::new(0),
            cost_per_req_hour: AtomicU64::new(0),
        })
    }

    pub fn record(&self, model_id: &str) {
        let cost = cost_per_request(model_id);
        let cost_bits = cost.to_bits();
        self.requests_today.fetch_add(1, Ordering::Relaxed);
        self.requests_hour.fetch_add(1, Ordering::Relaxed);
        // accumulate cost as sum of f64 bits via fetch_add on u64
        // We store micro-dollars (cost * 1e9) as integer to avoid float atomics
        let micro = (cost * 1e9) as u64;
        self.cost_per_req_today.fetch_add(micro, Ordering::Relaxed);
        self.cost_per_req_hour.fetch_add(micro, Ordering::Relaxed);
        let _ = cost_bits; // suppress unused warning
    }

    pub fn cost_today(&self, _model_id: &str) -> f64 {
        let nano = self.cost_per_req_today.load(Ordering::Relaxed);
        nano as f64 / 1e9
    }

    pub fn cost_hour(&self, _model_id: &str) -> f64 {
        let nano = self.cost_per_req_hour.load(Ordering::Relaxed);
        nano as f64 / 1e9
    }

    pub fn reset_hour(&self) {
        self.requests_hour.store(0, Ordering::Relaxed);
        self.cost_per_req_hour.store(0, Ordering::Relaxed);
    }

    pub fn requests_today(&self) -> u64 {
        self.requests_today.load(Ordering::Relaxed)
    }

    pub fn requests_hour(&self) -> u64 {
        self.requests_hour.load(Ordering::Relaxed)
    }
}

impl Default for CostMonitor {
    fn default() -> Self {
        Self {
            requests_today: AtomicU64::new(0),
            requests_hour: AtomicU64::new(0),
            cost_per_req_today: AtomicU64::new(0),
            cost_per_req_hour: AtomicU64::new(0),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_count_increments() {
        let m = CostMonitor::new();
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        assert_eq!(m.requests_today(), 2);
        assert_eq!(m.requests_hour(), 2);
    }

    #[test]
    fn test_cost_calculation() {
        let m = CostMonitor::new();
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        let cost = m.cost_today("@cf/qwen/qwen3-30b-a3b-fp8");
        let expected = COST_QWEN3_30B;
        assert!(
            (cost - expected).abs() < 1e-9,
            "cost={cost}, expected={expected}"
        );
    }

    #[test]
    fn test_multimodel() {
        let m = CostMonitor::new();
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        m.record("@cf/meta/llama-3-8b-instruct");
        let cost = m.cost_today("");
        let expected = COST_QWEN3_30B + COST_LLAMA_8B_FAST;
        assert!(
            (cost - expected).abs() < 1e-9,
            "cost={cost}, expected={expected}"
        );
    }

    #[test]
    fn test_thread_safe() {
        use std::thread;
        let m = CostMonitor::new();
        let handles: Vec<_> = (0..10)
            .map(|_| {
                let m2 = Arc::clone(&m);
                thread::spawn(move || {
                    m2.record("@cf/qwen/qwen3-30b-a3b-fp8");
                })
            })
            .collect();
        for h in handles {
            h.join().unwrap();
        }
        assert_eq!(m.requests_today(), 10);
    }

    #[test]
    fn test_reset_hour() {
        let m = CostMonitor::new();
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        m.record("@cf/qwen/qwen3-30b-a3b-fp8");
        assert_eq!(m.requests_hour(), 2);
        m.reset_hour();
        assert_eq!(m.requests_hour(), 0);
        assert_eq!(m.cost_hour(""), 0.0);
        // today counter unchanged
        assert_eq!(m.requests_today(), 2);
    }
}

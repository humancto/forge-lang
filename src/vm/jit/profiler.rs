use std::collections::HashMap;

const HOT_THRESHOLD: u32 = 100;

/// Tracks function call counts to detect hot functions for JIT compilation.
pub struct Profiler {
    counts: HashMap<String, u32>,
}

impl Profiler {
    pub fn new() -> Self {
        Self {
            counts: HashMap::new(),
        }
    }

    pub fn record_call(&mut self, name: &str) {
        let count = self.counts.entry(name.to_string()).or_insert(0);
        *count = count.saturating_add(1);
    }

    pub fn is_hot(&self, name: &str) -> bool {
        self.counts.get(name).copied().unwrap_or(0) >= HOT_THRESHOLD
    }

    pub fn call_count(&self, name: &str) -> u32 {
        self.counts.get(name).copied().unwrap_or(0)
    }
}

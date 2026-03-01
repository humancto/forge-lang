use std::collections::HashMap;
use std::time::{Duration, Instant};

const HOT_THRESHOLD: u32 = 100;

#[derive(Debug, Clone)]
pub struct FunctionStats {
    pub call_count: u32,
    pub total_time: Duration,
}

impl FunctionStats {
    fn new() -> Self {
        Self {
            call_count: 0,
            total_time: Duration::ZERO,
        }
    }
}

pub struct Profiler {
    stats: HashMap<String, FunctionStats>,
    call_stack: Vec<(String, Instant)>,
    enabled: bool,
}

impl Profiler {
    pub fn new(enabled: bool) -> Self {
        Self {
            stats: HashMap::new(),
            call_stack: Vec::new(),
            enabled,
        }
    }

    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    pub fn enter_function(&mut self, name: &str) {
        if !self.enabled {
            return;
        }
        let entry = self
            .stats
            .entry(name.to_string())
            .or_insert_with(FunctionStats::new);
        entry.call_count = entry.call_count.saturating_add(1);
        self.call_stack.push((name.to_string(), Instant::now()));
    }

    pub fn exit_function(&mut self) {
        if !self.enabled {
            return;
        }
        if let Some((name, start)) = self.call_stack.pop() {
            let elapsed = start.elapsed();
            if let Some(entry) = self.stats.get_mut(&name) {
                entry.total_time += elapsed;
            }
        }
    }

    pub fn is_hot(&self, name: &str) -> bool {
        self.stats
            .get(name)
            .map(|s| s.call_count >= HOT_THRESHOLD)
            .unwrap_or(false)
    }

    pub fn call_count(&self, name: &str) -> u32 {
        self.stats.get(name).map(|s| s.call_count).unwrap_or(0)
    }

    pub fn report(&self) -> Vec<(&str, &FunctionStats)> {
        let mut entries: Vec<(&str, &FunctionStats)> =
            self.stats.iter().map(|(k, v)| (k.as_str(), v)).collect();
        entries.sort_by(|a, b| b.1.total_time.cmp(&a.1.total_time));
        entries
    }

    pub fn print_report(&self) {
        let entries = self.report();
        if entries.is_empty() {
            println!("  No function calls recorded.");
            return;
        }

        println!();
        println!("  \x1B[1mProfile Report\x1B[0m");
        println!("  {:-<60}", "");
        println!(
            "  {:<30} {:>8} {:>12} {:>8}",
            "Function", "Calls", "Total (ms)", "Avg (µs)"
        );
        println!("  {:-<60}", "");

        for (name, stats) in &entries {
            let total_ms = stats.total_time.as_secs_f64() * 1000.0;
            let avg_us = if stats.call_count > 0 {
                stats.total_time.as_secs_f64() * 1_000_000.0 / stats.call_count as f64
            } else {
                0.0
            };

            let hot_marker = if stats.call_count >= HOT_THRESHOLD {
                "\x1B[31m*\x1B[0m"
            } else {
                " "
            };

            println!(
                "  {}{:<29} {:>8} {:>11.2} {:>7.1}",
                hot_marker, name, stats.call_count, total_ms, avg_us
            );
        }

        println!("  {:-<60}", "");

        let hot_count = entries
            .iter()
            .filter(|(_, s)| s.call_count >= HOT_THRESHOLD)
            .count();
        if hot_count > 0 {
            println!(
                "  \x1B[31m*\x1B[0m {} hot function{} (>={} calls)",
                hot_count,
                if hot_count == 1 { "" } else { "s" },
                HOT_THRESHOLD
            );
        }
        println!();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiler_disabled_does_nothing() {
        let mut p = Profiler::new(false);
        p.enter_function("foo");
        p.exit_function();
        assert_eq!(p.call_count("foo"), 0);
        assert!(!p.is_hot("foo"));
    }

    #[test]
    fn profiler_tracks_calls() {
        let mut p = Profiler::new(true);
        for _ in 0..5 {
            p.enter_function("bar");
            p.exit_function();
        }
        assert_eq!(p.call_count("bar"), 5);
        assert!(!p.is_hot("bar"));
    }

    #[test]
    fn profiler_hot_detection() {
        let mut p = Profiler::new(true);
        for _ in 0..100 {
            p.enter_function("hot_fn");
            p.exit_function();
        }
        assert!(p.is_hot("hot_fn"));
        assert_eq!(p.call_count("hot_fn"), 100);
    }

    #[test]
    fn profiler_multiple_functions() {
        let mut p = Profiler::new(true);
        for _ in 0..10 {
            p.enter_function("a");
            p.exit_function();
        }
        for _ in 0..20 {
            p.enter_function("b");
            p.exit_function();
        }
        assert_eq!(p.call_count("a"), 10);
        assert_eq!(p.call_count("b"), 20);
    }

    #[test]
    fn profiler_report_sorted_by_time() {
        let mut p = Profiler::new(true);
        p.enter_function("fast");
        p.exit_function();

        p.enter_function("slow");
        std::thread::sleep(std::time::Duration::from_millis(2));
        p.exit_function();

        let report = p.report();
        assert_eq!(report.len(), 2);
        assert_eq!(report[0].0, "slow");
        assert_eq!(report[1].0, "fast");
    }

    #[test]
    fn profiler_tracks_timing() {
        let mut p = Profiler::new(true);
        p.enter_function("timed");
        std::thread::sleep(std::time::Duration::from_millis(5));
        p.exit_function();

        let stats = p.stats.get("timed").unwrap();
        assert!(stats.total_time.as_millis() >= 4);
    }

    #[test]
    fn profiler_unknown_function() {
        let p = Profiler::new(true);
        assert_eq!(p.call_count("unknown"), 0);
        assert!(!p.is_hot("unknown"));
    }

    #[test]
    fn profiler_saturating_add() {
        let mut p = Profiler::new(true);
        p.stats.insert(
            "overflow".to_string(),
            FunctionStats {
                call_count: u32::MAX,
                total_time: Duration::ZERO,
            },
        );
        p.enter_function("overflow");
        p.exit_function();
        assert_eq!(p.call_count("overflow"), u32::MAX);
    }
}

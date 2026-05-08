//! Resource Monitor & Adaptive Throttling
//!
//! Provides real-time system resource monitoring using `sysinfo` and
//! adaptive throttling based on system load. Agents can query limits
//! to self-regulate their resource usage.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use sysinfo::System;

const REFRESH_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Debug, Clone)]
pub struct ResourceLimits {
    pub max_concurrent_agents: usize,
    pub max_tasks_per_agent: usize,
    pub ram_threshold_mb: u64,
    pub cpu_threshold_pct: f64,
    pub throttle_delay_ms: u64,
    pub priority_boost: bool,
}

impl Default for ResourceLimits {
    fn default() -> Self {
        Self {
            max_concurrent_agents: 20,
            max_tasks_per_agent: 5,
            ram_threshold_mb: 512,
            cpu_threshold_pct: 90.0,
            throttle_delay_ms: 50,
            priority_boost: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ResourceSnapshot {
    pub cpu_cores: usize,
    pub cpu_usage_pct: f64,
    pub ram_total_mb: u64,
    pub ram_used_mb: u64,
    pub ram_free_mb: u64,
    pub load_level: LoadLevel,
    pub timestamp: Instant,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum LoadLevel {
    Idle,
    Normal,
    Busy,
    Critical,
}

impl LoadLevel {
    pub fn from_usage(cpu_pct: f64, ram_pct: f64) -> Self {
        let max_pct = cpu_pct.max(ram_pct);
        match max_pct {
            p if p < 30.0 => LoadLevel::Idle,
            p if p < 70.0 => LoadLevel::Normal,
            p if p < 90.0 => LoadLevel::Busy,
            _ => LoadLevel::Critical,
        }
    }
}

pub struct ResourceMonitor {
    limits: Arc<Mutex<ResourceLimits>>,
    last_snapshot: Arc<Mutex<Option<ResourceSnapshot>>>,
    last_refresh: Arc<Mutex<Instant>>,
}

impl ResourceMonitor {
    pub fn new() -> Self {
        Self {
            limits: Arc::new(Mutex::new(ResourceLimits::default())),
            last_snapshot: Arc::new(Mutex::new(None)),
            last_refresh: Arc::new(Mutex::new(Instant::now() - REFRESH_INTERVAL)),
        }
    }

    pub fn set_limits(&self, limits: ResourceLimits) {
        *self.limits.lock().unwrap() = limits;
    }

    pub fn get_limits(&self) -> ResourceLimits {
        self.limits.lock().unwrap().clone()
    }

    pub fn snapshot(&self) -> ResourceSnapshot {
        if self.should_refresh() {
            self.refresh();
        }
        self.last_snapshot.lock().unwrap().clone().unwrap_or_else(|| {
            ResourceSnapshot {
                cpu_cores: 0,
                cpu_usage_pct: 0.0,
                ram_total_mb: 0,
                ram_used_mb: 0,
                ram_free_mb: 0,
                load_level: LoadLevel::Idle,
                timestamp: Instant::now(),
            }
        })
    }

    fn should_refresh(&self) -> bool {
        let last = *self.last_refresh.lock().unwrap();
        last.elapsed() >= REFRESH_INTERVAL
    }

    fn refresh(&self) {
        let mut sys = System::new_all();
        sys.refresh_all();

        let cpu_usage: f64 = sys.cpus().iter()
            .map(|c| c.cpu_usage() as f64)
            .sum::<f64>() / sys.cpus().len().max(1) as f64;

        let ram_total = sys.total_memory() / 1024 / 1024;
        let ram_used = sys.used_memory() / 1024 / 1024;
        let ram_free = sys.free_memory() / 1024 / 1024;
        let ram_pct = if ram_total > 0 { (ram_used as f64 / ram_total as f64) * 100.0 } else { 0.0 };

        let snapshot = ResourceSnapshot {
            cpu_cores: sys.cpus().len(),
            cpu_usage_pct: (cpu_usage * 10.0).round() / 10.0,
            ram_total_mb: ram_total,
            ram_used_mb: ram_used,
            ram_free_mb: ram_free,
            load_level: LoadLevel::from_usage(cpu_usage, ram_pct),
            timestamp: Instant::now(),
        };

        *self.last_snapshot.lock().unwrap() = Some(snapshot);
        *self.last_refresh.lock().unwrap() = Instant::now();
    }

    /// Check if system can accept a new agent based on current load
    pub fn can_accept_agent(&self, current_agents: usize) -> bool {
        let limits = self.get_limits();
        if current_agents >= limits.max_concurrent_agents {
            return false;
        }
        let snap = self.snapshot();
        snap.load_level != LoadLevel::Critical
    }

    /// Get recommended throttle delay based on current load
    pub fn throttle_delay(&self) -> Duration {
        let limits = self.get_limits();
        let snap = self.snapshot();
        let base = Duration::from_millis(limits.throttle_delay_ms);
        match snap.load_level {
            LoadLevel::Idle => Duration::from_millis(0),
            LoadLevel::Normal => base,
            LoadLevel::Busy => base * 5,
            LoadLevel::Critical => base * 20,
        }
    }

    /// Asset whether a task should be executed based on priority scheduling
    pub fn should_execute_task(&self, priority: u8, current_load_tasks: usize) -> bool {
        let limits = self.get_limits();
        let snap = self.snapshot();

        match snap.load_level {
            LoadLevel::Idle | LoadLevel::Normal => current_load_tasks < limits.max_tasks_per_agent,
            LoadLevel::Busy => current_load_tasks < limits.max_tasks_per_agent / 2 || priority >= 7,
            LoadLevel::Critical => current_load_tasks == 0 || priority >= 9,
        }
    }
}

impl Default for ResourceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_level_ordering() {
        assert!(LoadLevel::Idle < LoadLevel::Normal);
        assert!(LoadLevel::Normal < LoadLevel::Busy);
        assert!(LoadLevel::Busy < LoadLevel::Critical);
    }

    #[test]
    fn test_load_level_from_usage() {
        assert_eq!(LoadLevel::from_usage(10.0, 20.0), LoadLevel::Idle);
        assert_eq!(LoadLevel::from_usage(50.0, 40.0), LoadLevel::Normal);
        assert_eq!(LoadLevel::from_usage(85.0, 60.0), LoadLevel::Busy);
        assert_eq!(LoadLevel::from_usage(95.0, 10.0), LoadLevel::Critical);
    }

    #[test]
    fn test_resource_limits_default() {
        let limits = ResourceLimits::default();
        assert_eq!(limits.max_concurrent_agents, 20);
        assert_eq!(limits.max_tasks_per_agent, 5);
    }

    #[test]
    fn test_throttle_delay() {
        let monitor = ResourceMonitor::new();
        let delay = monitor.throttle_delay();
        assert!(delay.as_millis() >= 0);
    }
}

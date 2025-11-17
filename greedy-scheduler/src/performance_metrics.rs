/// Performance metrics for the greedy scheduler
///
/// Tracks internal performance from TPU message ingestion through execution
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, Default)]
pub struct PerformanceMetrics {
    // TPU ingestion metrics
    pub tpu_messages_received: u64,
    pub tpu_transactions_received: u64,
    pub tpu_total_processing_time_ns: u64,

    // Check phase metrics
    pub transactions_sent_to_check: u64,
    pub transactions_checked: u64,
    pub check_total_processing_time_ns: u64,

    // Execution phase metrics
    pub transactions_sent_to_execute: u64,
    pub transactions_executed: u64,
    pub execution_total_processing_time_ns: u64,

    // Overall metrics
    pub total_transactions_processed: u64,
    pub start_time_ns: u64,

    // Queue depths (snapshots)
    pub unchecked_queue_depth: usize,
    pub checked_queue_depth: usize,
    pub in_flight_cost: u32,
}

impl PerformanceMetrics {
    /// Creates new metrics with current timestamp
    pub fn new() -> Self {
        Self { start_time_ns: current_time_ns(), ..Default::default() }
    }

    /// Records TPU message processing
    pub fn record_tpu_ingestion(&mut self, tx_count: usize, duration: Duration) {
        self.tpu_messages_received += 1;
        self.tpu_transactions_received += tx_count as u64;
        self.tpu_total_processing_time_ns += duration.as_nanos() as u64;
    }

    /// Records check phase
    pub fn record_check_sent(&mut self, count: usize) {
        self.transactions_sent_to_check += count as u64;
    }

    /// Records check completion
    pub fn record_check_completed(&mut self, duration: Duration) {
        self.transactions_checked += 1;
        self.check_total_processing_time_ns += duration.as_nanos() as u64;
    }

    /// Records execution phase
    pub fn record_execution_sent(&mut self, count: usize) {
        self.transactions_sent_to_execute += count as u64;
    }

    /// Records execution completion
    pub fn record_execution_completed(&mut self, duration: Duration) {
        self.transactions_executed += 1;
        self.execution_total_processing_time_ns += duration.as_nanos() as u64;
        self.total_transactions_processed += 1;
    }

    /// Updates queue depth snapshots
    pub fn update_queue_depths(&mut self, unchecked: usize, checked: usize, in_flight_cost: u32) {
        self.unchecked_queue_depth = unchecked;
        self.checked_queue_depth = checked;
        self.in_flight_cost = in_flight_cost;
    }

    /// Calculates TPU ingestion rate (transactions per second)
    pub fn tpu_ingestion_tps(&self) -> f64 {
        let elapsed = self.elapsed_seconds();
        if elapsed > 0.0 { self.tpu_transactions_received as f64 / elapsed } else { 0.0 }
    }

    /// Calculates execution rate (transactions per second)
    pub fn execution_tps(&self) -> f64 {
        let elapsed = self.elapsed_seconds();
        if elapsed > 0.0 { self.transactions_executed as f64 / elapsed } else { 0.0 }
    }

    /// Average TPU processing time per transaction (microseconds)
    pub fn avg_tpu_processing_time_us(&self) -> f64 {
        if self.tpu_transactions_received > 0 {
            (self.tpu_total_processing_time_ns as f64 / self.tpu_transactions_received as f64)
                / 1000.0
        } else {
            0.0
        }
    }

    /// Average check processing time per transaction (microseconds)
    pub fn avg_check_processing_time_us(&self) -> f64 {
        if self.transactions_checked > 0 {
            (self.check_total_processing_time_ns as f64 / self.transactions_checked as f64) / 1000.0
        } else {
            0.0
        }
    }

    /// Average execution time per transaction (microseconds)
    pub fn avg_execution_time_us(&self) -> f64 {
        if self.transactions_executed > 0 {
            (self.execution_total_processing_time_ns as f64 / self.transactions_executed as f64)
                / 1000.0
        } else {
            0.0
        }
    }

    /// Check pass-through rate (checked / sent_to_check)
    pub fn check_pass_rate(&self) -> f64 {
        if self.transactions_sent_to_check > 0 {
            self.transactions_checked as f64 / self.transactions_sent_to_check as f64
        } else {
            0.0
        }
    }

    /// Execution completion rate (executed / sent_to_execute)
    pub fn execution_completion_rate(&self) -> f64 {
        if self.transactions_sent_to_execute > 0 {
            self.transactions_executed as f64 / self.transactions_sent_to_execute as f64
        } else {
            0.0
        }
    }

    /// Overall processing efficiency (executed / received)
    pub fn overall_efficiency(&self) -> f64 {
        if self.tpu_transactions_received > 0 {
            self.transactions_executed as f64 / self.tpu_transactions_received as f64
        } else {
            0.0
        }
    }

    /// Time elapsed since metrics started (seconds)
    pub fn elapsed_seconds(&self) -> f64 {
        let now = current_time_ns();
        (now - self.start_time_ns) as f64 / 1_000_000_000.0
    }

    /// Resets all counters but keeps start time
    pub fn reset(&mut self) {
        *self = Self { start_time_ns: self.start_time_ns, ..Default::default() };
    }
}

/// Helper to get current time in nanoseconds
fn current_time_ns() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

/// Snapshot of current performance metrics for HTTP API
#[derive(Debug, Clone, serde::Serialize)]
pub struct PerformanceSnapshot {
    // Throughput metrics
    pub tpu_ingestion_tps: f64,
    pub execution_tps: f64,

    // Latency metrics (microseconds)
    pub avg_tpu_processing_time_us: f64,
    pub avg_check_processing_time_us: f64,
    pub avg_execution_time_us: f64,

    // Efficiency metrics
    pub check_pass_rate: f64,
    pub execution_completion_rate: f64,
    pub overall_efficiency: f64,

    // Volume metrics
    pub tpu_messages_received: u64,
    pub tpu_transactions_received: u64,
    pub transactions_sent_to_check: u64,
    pub transactions_checked: u64,
    pub transactions_sent_to_execute: u64,
    pub transactions_executed: u64,
    pub total_transactions_processed: u64,

    // Queue depths
    pub unchecked_queue_depth: usize,
    pub checked_queue_depth: usize,
    pub in_flight_cost: u32,

    // Timing
    pub elapsed_seconds: f64,
}

impl From<&PerformanceMetrics> for PerformanceSnapshot {
    fn from(metrics: &PerformanceMetrics) -> Self {
        Self {
            tpu_ingestion_tps: metrics.tpu_ingestion_tps(),
            execution_tps: metrics.execution_tps(),
            avg_tpu_processing_time_us: metrics.avg_tpu_processing_time_us(),
            avg_check_processing_time_us: metrics.avg_check_processing_time_us(),
            avg_execution_time_us: metrics.avg_execution_time_us(),
            check_pass_rate: metrics.check_pass_rate(),
            execution_completion_rate: metrics.execution_completion_rate(),
            overall_efficiency: metrics.overall_efficiency(),
            tpu_messages_received: metrics.tpu_messages_received,
            tpu_transactions_received: metrics.tpu_transactions_received,
            transactions_sent_to_check: metrics.transactions_sent_to_check,
            transactions_checked: metrics.transactions_checked,
            transactions_sent_to_execute: metrics.transactions_sent_to_execute,
            transactions_executed: metrics.transactions_executed,
            total_transactions_processed: metrics.total_transactions_processed,
            unchecked_queue_depth: metrics.unchecked_queue_depth,
            checked_queue_depth: metrics.checked_queue_depth,
            in_flight_cost: metrics.in_flight_cost,
            elapsed_seconds: metrics.elapsed_seconds(),
        }
    }
}

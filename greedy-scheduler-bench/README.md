# Greedy Scheduler Benchmark

Performance benchmarks for the greedy scheduler using Criterion.

## Available Benchmarks

### 1. TPU Ingestion (`tpu_ingestion`)
Measures the throughput of TPU message ingestion in the greedy scheduler.

**Metrics:**
- Time to process batches of 10, 50, 100, 500, and 1000 transactions
- Throughput in transactions per second

**What it measures:**
- Performance of `drain_tpu()` function
- Transaction parsing and validation overhead
- Initial priority calculation

### 2. Priority Calculation (`priority_calculation`)
Measures the performance of transaction priority calculation.

**Metrics:**
- Time to calculate priorities for 10, 50, and 100 transactions
- Transactions processed per second

**What it measures:**
- Performance of `calculate_priority()`
- Fee and compute budget calculation cost
- Priority signers/programs identification overhead

### 3. Full Poll Cycle (`full_poll_cycle`)
Measures the complete poll cycle including ingestion and check scheduling.

**Metrics:**
- Total time of a poll cycle with 10, 50, 100, and 200 transactions
- Overall system throughput

**What it measures:**
- Performance of `poll()` function
- Component integration
- Worker communication overhead

### 4. Queue Operations (`queue_operations`)
Measures the overhead of queue management (unchecked/checked).

**Metrics:**
- Time for multiple poll cycles with 50, 100, 200, and 500 transactions
- Queue operations performance under load

**What it measures:**
- Performance of `MinMaxHeap` operations (push/pop)
- Eviction overhead when queues are full
- Prioritization efficiency

### 5. Priority List Operations (`priority_list`)
Measures add and remove operations for signers in the priority list.

**Metrics:**
- Time to add and remove 10, 50, and 100 signers
- Operations per second

**What it measures:**
- Performance of `add_priority_signer_with_quota()`
- Performance of `remove_priority_signer()`
- HashMap operations overhead

### 6. Concurrent Operations (`concurrent_operations`)
Simulates concurrent ingestion and scheduling operations (as leader).

**Metrics:**
- Time to process 50, 100, and 200 transactions with active scheduling
- Throughput under realistic conditions

**What it measures:**
- Performance when scheduler is leader
- Lock checking overhead
- Complete pipeline integration

### 7. Leader Priority Execution (`leader_priority_execution`) 🆕
**Complete execution pipeline with active priority list (IS_LEADER).**

**Metrics:**
- Time to process 50, 100, 200, and 500 mixed transactions (priority + non-priority)
- Throughput of complete cycle: ingestion + check + scheduling + execution
- Performance with 50% priority transactions (10x multiplier)

**What it measures:**
- Real scheduler performance as leader
- Priority list impact on total throughput
- Prioritization algorithm efficiency under load
- Complete pipeline: TPU → Check → Execute
- Quota tracking and statistics overhead

**Configuration:**
- 50% transactions with priority signers
- 5 configured priority programs
- Priority multiplier: 10x
- Quotas: 10000 per signer/program
- 5 poll cycles to simulate complete processing

### 8. Priority Calculation with Boost (`priority_calculation_with_boost`) 🆕
**Priority calculation with priority list boost.**

**Metrics:**
- Time to calculate priorities for 50, 100, and 200 transactions
- 1/3 transactions with priority boost (100x multiplier)
- Calculation throughput with active priority list

**What it measures:**
- Priority calculation overhead with boost
- Priority list lookup performance
- High multiplier impact on processing time
- Priority signers/programs identification efficiency

**Configuration:**
- 1/3 transactions with priority signers
- Priority multiplier: 100x (aggressive boost)
- Quotas: 5000 per signer
- 2/3 normal transactions for comparison

## How to Run

### Run all benchmarks:
```bash
cargo bench --package greedy-scheduler-bench
```

### Run a specific benchmark:
```bash
# TPU ingestion
cargo bench --package greedy-scheduler-bench tpu_ingestion

# Priority calculation
cargo bench --package greedy-scheduler-bench priority_calculation

# Full poll cycle
cargo bench --package greedy-scheduler-bench full_poll_cycle

# Queue operations
cargo bench --package greedy-scheduler-bench queue_operations

# Priority list operations
cargo bench --package greedy-scheduler-bench priority_list

# Concurrent operations
cargo bench --package greedy-scheduler-bench concurrent_operations

# Leader with priority execution (NEW)
cargo bench --package greedy-scheduler-bench leader_priority_execution

# Priority calculation with boost (NEW)
cargo bench --package greedy-scheduler-bench priority_calculation_with_boost
```

### Run with specific batch size:
```bash
# Only 100 transaction batches
cargo bench --package greedy-scheduler-bench -- --bench-name "100"
```

## Results

Results are saved in `target/criterion/` with:
- Interactive HTML reports
- Historical performance data
- Comparison graphs between runs
- Detailed statistics (mean, median, standard deviation)

To view results:
```bash
open target/criterion/report/index.html
```

## Interpreting Results

### Important Metrics:

1. **Throughput**: Transactions/second processed
   - Higher values = better performance
   - Compare across different batch sizes

2. **Latency**: Time per operation
   - Lower values = better performance
   - Observe 95th percentile for worst-case

3. **Scalability**: How performance changes with load
   - Linear performance is ideal
   - Identify bottlenecks where performance degrades

### Bottleneck Analysis:

- **slow tpu_ingestion**: Issue with transaction parsing or validation
- **slow priority_calculation**: Optimize fee/priority calculation
- **slow queue_operations**: Consider alternative data structures
- **slow concurrent_operations**: Contention or lock issues

## Configuration

To modify benchmark settings, edit `benches/scheduler_throughput.rs`:

```rust
// Batch sizes to test
for batch_size in [10, 50, 100, 500, 1000].iter() {
    // ...
}

// Harness configuration
let logon = ClientLogon {
    worker_count: 4,
    allocator_size: 64 * 1024 * 1024, // 64MB
    // ...
};
```

## Comparing with Previous Versions

Criterion maintains benchmark history. To compare with previous version:

```bash
# Save baseline
cargo bench --package greedy-scheduler-bench -- --save-baseline main

# Make code changes...

# Compare with baseline
cargo bench --package greedy-scheduler-bench -- --baseline main
```

## Suggested Optimizations

Based on benchmark results, consider:

1. **If tpu_ingestion is slow:**
   - Batch transaction parsing
   - Cache validation results
   - Lazy priority computation

2. **If priority_calculation is slow:**
   - Pre-compute static values
   - Simplify fee calculation
   - Use approximations when appropriate

3. **If queue_operations is slow:**
   - Adjust queue capacities
   - Optimize eviction algorithm
   - Consider lock-free data structures

4. **If concurrent_operations is slow:**
   - Reduce lock contention
   - Increase parallelism
   - Optimize component communication

## Performance Goals

Target metrics for production:
- **TPU Ingestion**: > 10,000 TPS
- **Priority Calculation**: > 50,000 calculations/sec
- **Full Poll Cycle**: < 10ms for 100 transactions
- **Queue Operations**: < 1µs per operation
- **Leader Execution**: > 5,000 TPS end-to-end

## CI Integration

Benchmarks can be run in CI to detect performance regressions:

```bash
# Fail if performance degrades by more than 10%
cargo bench --package greedy-scheduler-bench -- --baseline main --save-baseline ci
```

## Profiling

For detailed profiling, use:

```bash
# Generate flamegraph
cargo bench --package greedy-scheduler-bench -- --profile-time=5

# With perf (Linux only)
perf record -g cargo bench --package greedy-scheduler-bench
perf report
```

## Troubleshooting

### Inconsistent Results
- Close other applications
- Disable CPU frequency scaling
- Run multiple iterations: `--sample-size 1000`

### Out of Memory
- Reduce batch sizes in benchmarks
- Decrease allocator size
- Run benchmarks individually

### Compilation Issues
- Ensure all dependencies are up to date
- Clean build: `cargo clean`
- Check Rust version: `rustc --version`

use agave_scheduler_bindings::{
    IS_LEADER, IS_NOT_LEADER, ProgressMessage, SharableTransactionRegion, TpuToPackMessage,
};
use agave_scheduling_utils::handshake::client::ClientSession;
use agave_scheduling_utils::handshake::server::AgaveSession;
use agave_scheduling_utils::handshake::{self, ClientLogon};
use criterion::{BenchmarkId, Criterion, Throughput, black_box, criterion_group, criterion_main};
use greedy_scheduler::{GreedyQueues, GreedyScheduler};
use rts_alloc::Allocator;
use solana_compute_budget_interface::ComputeBudgetInstruction;
use solana_hash::Hash;
use solana_keypair::{Keypair, Signer};
use solana_transaction::Transaction;
use solana_transaction::versioned::VersionedTransaction;
use std::os::fd::IntoRawFd;
use std::ptr::NonNull;

const MOCK_PROGRESS_NOT_LEADER: ProgressMessage = ProgressMessage {
    leader_state: IS_NOT_LEADER,
    current_slot: 10,
    next_leader_slot: 11,
    leader_range_end: 11,
    remaining_cost_units: 50_000_000,
    current_slot_progress: 25,
};

const MOCK_PROGRESS_LEADER: ProgressMessage = ProgressMessage {
    leader_state: IS_LEADER,
    current_slot: 10,
    next_leader_slot: 11,
    leader_range_end: 11,
    remaining_cost_units: 48_000_000,
    current_slot_progress: 50,
};

struct BenchHarness {
    session: AgaveSession,
    scheduler: GreedyScheduler,
    queues: GreedyQueues,
}

impl BenchHarness {
    fn setup() -> Self {
        let logon = ClientLogon {
            worker_count: 4,
            allocator_size: 64 * 1024 * 1024, // 64MB for better performance
            allocator_handles: 1,
            tpu_to_pack_capacity: 4096,
            progress_tracker_capacity: 256,
            pack_to_worker_capacity: 2048,
            worker_to_pack_capacity: 2048,
            flags: 0,
        };
        let (agave_session, files) = handshake::server::Server::setup_session(logon).unwrap();
        let client_session = handshake::client::setup_session(
            &logon,
            files.into_iter().map(IntoRawFd::into_raw_fd).collect(),
        )
        .unwrap();
        let (scheduler, queues) = GreedyScheduler::new(client_session);

        Self { session: agave_session, scheduler, queues }
    }

    fn allocator(&self) -> &Allocator {
        &self.session.tpu_to_pack.allocator
    }

    fn send_progress(&mut self, progress: ProgressMessage) {
        self.session.progress_tracker.sync();
        self.session.progress_tracker.try_write(progress).unwrap();
        self.session.progress_tracker.commit();
    }

    fn send_tx(&mut self, tx: &VersionedTransaction) {
        let mut packet = bincode::serialize(&tx).unwrap();
        let packet_len = packet.len().try_into().unwrap();
        let pointer = self.allocator().allocate(packet_len).unwrap();
        unsafe {
            pointer
                .copy_from_nonoverlapping(NonNull::new(packet.as_mut_ptr()).unwrap(), packet.len());
        }
        let offset = unsafe { self.allocator().offset(pointer) };
        let tx = SharableTransactionRegion { offset, length: packet_len };

        self.session.tpu_to_pack.producer.sync();
        self.session
            .tpu_to_pack
            .producer
            .try_write(TpuToPackMessage { transaction: tx, flags: 0, src_addr: [0; 16] })
            .unwrap();
        self.session.tpu_to_pack.producer.commit();
    }

    fn poll(&mut self) {
        self.scheduler.poll(&mut self.queues);
    }
}

fn create_simple_transfer(payer: &Keypair) -> VersionedTransaction {
    let to = solana_pubkey::Pubkey::new_unique();
    solana_system_transaction::transfer(payer, &to, 1, Hash::new_unique()).into()
}

fn create_tx_with_budget(payer: &Keypair, cu_limit: u32, cu_price: u64) -> VersionedTransaction {
    Transaction::new_signed_with_payer(
        &[
            ComputeBudgetInstruction::set_compute_unit_limit(cu_limit),
            ComputeBudgetInstruction::set_compute_unit_price(cu_price),
        ],
        Some(&payer.pubkey()),
        &[payer],
        Hash::new_from_array([1; 32]),
    )
    .into()
}

// Benchmark: TPU message ingestion throughput
fn bench_tpu_ingestion(c: &mut Criterion) {
    let mut group = c.benchmark_group("tpu_ingestion");

    for batch_size in [10, 50, 100, 500, 1000].iter() {
        group.throughput(Throughput::Elements(*batch_size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(batch_size), batch_size, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();
                    harness.send_progress(MOCK_PROGRESS_NOT_LEADER);

                    // Pre-generate transactions
                    let txs: Vec<_> = (0..size)
                        .map(|_| {
                            let payer = Keypair::new();
                            create_simple_transfer(&payer)
                        })
                        .collect();

                    (harness, txs)
                },
                |(mut harness, txs)| {
                    // Send all transactions
                    for tx in &txs {
                        harness.send_tx(tx);
                    }

                    // Poll to ingest
                    harness.poll();

                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Transaction priority calculation throughput
fn bench_priority_calculation(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_calculation");

    for num_txs in [10, 50, 100].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();
                    harness.send_progress(MOCK_PROGRESS_NOT_LEADER);

                    // Create transactions with varying priorities
                    let txs: Vec<_> = (0..size)
                        .map(|i| {
                            let payer = Keypair::new();
                            let cu_limit: u32 = 25_000 + (i * 1000);
                            let cu_price: u64 = 100u64 + (i as u64 * 50u64);
                            create_tx_with_budget(&payer, cu_limit, cu_price)
                        })
                        .collect();

                    (harness, txs)
                },
                |(mut harness, txs)| {
                    for tx in &txs {
                        harness.send_tx(tx);
                    }
                    harness.poll();
                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Full poll cycle (ingestion + check scheduling)
fn bench_full_poll_cycle(c: &mut Criterion) {
    let mut group = c.benchmark_group("full_poll_cycle");

    for num_txs in [10, 50, 100, 200].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();
                    harness.send_progress(MOCK_PROGRESS_NOT_LEADER);

                    let txs: Vec<_> = (0..size)
                        .map(|_| {
                            let payer = Keypair::new();
                            create_simple_transfer(&payer)
                        })
                        .collect();

                    // Pre-load transactions
                    for tx in &txs {
                        harness.send_tx(tx);
                    }

                    harness
                },
                |mut harness| {
                    // Measure the poll operation
                    harness.poll();
                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Queue management overhead
fn bench_queue_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("queue_operations");

    for num_txs in [50, 100, 200, 500].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();
                    harness.send_progress(MOCK_PROGRESS_NOT_LEADER);

                    let txs: Vec<_> = (0..size)
                        .map(|i| {
                            let payer = Keypair::new();
                            let cu_price = 100 + (i * 10);
                            create_tx_with_budget(&payer, 25_000, cu_price)
                        })
                        .collect();

                    for tx in &txs {
                        harness.send_tx(tx);
                    }
                    harness.poll();

                    harness
                },
                |mut harness| {
                    // Multiple poll cycles to test queue management
                    for _ in 0..5 {
                        harness.poll();
                    }
                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Priority list operations
fn bench_priority_list_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_list");

    for num_signers in [10, 50, 100].iter() {
        group.bench_with_input(
            BenchmarkId::new("add_remove_signers", num_signers),
            num_signers,
            |b, &size| {
                b.iter_batched(
                    || {
                        let mut harness = BenchHarness::setup();
                        let signers: Vec<_> = (0..size)
                            .map(|_| solana_pubkey::Pubkey::new_unique())
                            .collect();
                        (harness, signers)
                    },
                    |(mut harness, signers)| {
                        // Add signers
                        for signer in &signers {
                            harness
                                .scheduler
                                .add_priority_signer_with_quota(*signer, 1000);
                        }

                        // Remove signers
                        for signer in &signers {
                            harness.scheduler.remove_priority_signer(signer);
                        }

                        black_box(harness);
                    },
                    criterion::BatchSize::SmallInput,
                );
            },
        );
    }
    group.finish();
}

// Benchmark: Concurrent ingestion and scheduling
fn bench_concurrent_operations(c: &mut Criterion) {
    let mut group = c.benchmark_group("concurrent_operations");

    for num_txs in [50, 100, 200].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();

                    // Setup as leader
                    harness.send_progress(MOCK_PROGRESS_LEADER);

                    let txs: Vec<_> = (0..size)
                        .map(|i| {
                            let payer = Keypair::new();
                            let cu_price = 100 + (i * 20);
                            create_tx_with_budget(&payer, 25_000, cu_price)
                        })
                        .collect();

                    (harness, txs)
                },
                |(mut harness, txs)| {
                    // Simulate realistic operation: ingest, poll, schedule
                    for tx in &txs {
                        harness.send_tx(tx);
                    }

                    // Multiple poll cycles
                    for _ in 0..3 {
                        harness.poll();
                    }

                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Leader with priority list - Full execution pipeline
fn bench_leader_with_priority_list(c: &mut Criterion) {
    let mut group = c.benchmark_group("leader_priority_execution");

    for num_txs in [50, 100, 200, 500].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();

                    // Setup priority signers (50% of transactions will be priority)
                    let priority_signers: Vec<_> = (0..size / 2)
                        .map(|_| {
                            let keypair = Keypair::new();
                            let pubkey = keypair.pubkey();
                            (keypair, pubkey)
                        })
                        .collect();

                    // Add priority signers to scheduler
                    for (_, pubkey) in &priority_signers {
                        harness
                            .scheduler
                            .add_priority_signer_with_quota(*pubkey, 10000);
                    }

                    // Setup priority programs
                    let priority_programs: Vec<_> = (0..5)
                        .map(|_| solana_pubkey::Pubkey::new_unique())
                        .collect();

                    for program in &priority_programs {
                        harness
                            .scheduler
                            .add_priority_program_with_quota(*program, 10000);
                    }

                    // Set priority multiplier for boost
                    harness.scheduler.set_priority_multiplier(10);

                    // Setup as leader
                    harness.send_progress(MOCK_PROGRESS_LEADER);

                    // Create mixed transactions: priority and non-priority
                    let mut txs = Vec::new();

                    // Half priority transactions (using priority signers)
                    for (keypair, _) in &priority_signers {
                        let cu_price = 500 + (txs.len() as u64 * 50);
                        txs.push(create_tx_with_budget(keypair, 30_000, cu_price));
                    }

                    // Half non-priority transactions
                    for i in 0..(size - priority_signers.len()) {
                        let payer = Keypair::new();
                        let cu_price = 100 + (i as u64 * 20);
                        txs.push(create_tx_with_budget(&payer, 25_000, cu_price));
                    }

                    (harness, txs)
                },
                |(mut harness, txs)| {
                    // Full pipeline: ingestion + check + scheduling + execution

                    // Phase 1: Ingest all transactions
                    for tx in &txs {
                        harness.send_tx(tx);
                    }

                    // Phase 2: Poll to ingest and schedule checks
                    harness.poll();

                    // Phase 3: Process checks and schedule for execution
                    // (In real scenario, workers would respond with check results)
                    // Here we simulate multiple poll cycles as leader
                    for _ in 0..5 {
                        harness.poll();
                    }

                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

// Benchmark: Priority calculation with priority list boost
fn bench_priority_calculation_with_boost(c: &mut Criterion) {
    let mut group = c.benchmark_group("priority_calculation_with_boost");

    for num_txs in [50, 100, 200].iter() {
        group.throughput(Throughput::Elements(*num_txs as u64));
        group.bench_with_input(BenchmarkId::from_parameter(num_txs), num_txs, |b, &size| {
            b.iter_batched(
                || {
                    let mut harness = BenchHarness::setup();
                    harness.send_progress(MOCK_PROGRESS_NOT_LEADER);

                    // Create priority signers
                    let priority_signers: Vec<_> = (0..size / 3)
                        .map(|_| {
                            let keypair = Keypair::new();
                            let pubkey = keypair.pubkey();
                            (keypair, pubkey)
                        })
                        .collect();

                    // Add to priority list with high multiplier
                    for (_, pubkey) in &priority_signers {
                        harness
                            .scheduler
                            .add_priority_signer_with_quota(*pubkey, 5000);
                    }
                    harness.scheduler.set_priority_multiplier(100);

                    // Create mixed transactions
                    let mut txs = Vec::new();
                    let mut tx_index = 0;

                    // 1/3 priority transactions
                    for (keypair, _) in &priority_signers {
                        let cu_price = 1000 + (tx_index * 100);
                        txs.push(create_tx_with_budget(keypair, 40_000, cu_price));
                        tx_index += 1;
                    }

                    // 2/3 non-priority transactions
                    while txs.len() < size {
                        let payer = Keypair::new();
                        let cu_price = 100 + (tx_index * 10);
                        txs.push(create_tx_with_budget(&payer, 20_000, cu_price));
                        tx_index += 1;
                    }

                    (harness, txs)
                },
                |(mut harness, txs)| {
                    // Measure priority calculation with boost
                    for tx in &txs {
                        harness.send_tx(tx);
                    }
                    harness.poll();
                    black_box(harness);
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_tpu_ingestion,
    bench_priority_calculation,
    bench_full_poll_cycle,
    bench_queue_operations,
    bench_priority_list_operations,
    bench_concurrent_operations,
    bench_leader_with_priority_list,
    bench_priority_calculation_with_boost
);

criterion_main!(benches);

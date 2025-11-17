#[macro_use]
extern crate static_assertions;

pub mod api;
mod performance_metrics;
mod priority_list;
mod transaction_map;

use agave_feature_set::FeatureSet;
use agave_scheduler_bindings::pack_message_flags::check_flags;
use agave_scheduler_bindings::worker_message_types::{
    self, ExecutionResponse, fee_payer_balance_flags, not_included_reasons,
    parsing_and_sanitization_flags, resolve_flags, status_check_flags,
};
use agave_scheduler_bindings::{
    IS_LEADER, MAX_TRANSACTIONS_PER_MESSAGE, PackToWorkerMessage, ProgressMessage,
    SharableTransactionBatchRegion, SharableTransactionRegion, TpuToPackMessage,
    WorkerToPackMessage, pack_message_flags, processed_codes,
};
use agave_scheduling_utils::handshake::client::{ClientSession, ClientWorkerSession};
use agave_scheduling_utils::pubkeys_ptr::PubkeysPtr;
use agave_scheduling_utils::responses_region::{CheckResponsesPtr, ExecutionResponsesPtr};
use agave_scheduling_utils::transaction_ptr::{TransactionPtr, TransactionPtrBatch};
use agave_transaction_view::transaction_view::{SanitizedTransactionView, TransactionView};
use hashbrown::HashMap;
use min_max_heap::MinMaxHeap;
use rts_alloc::Allocator;
use solana_compute_budget_instruction::compute_budget_instruction_details;
use solana_cost_model::block_cost_limits::MAX_BLOCK_UNITS_SIMD_0256;
use solana_cost_model::cost_model::CostModel;
use solana_fee::FeeFeatures;
use solana_fee_structure::FeeBudgetLimits;
use solana_pubkey::Pubkey;
use solana_runtime_transaction::runtime_transaction::RuntimeTransaction;
use solana_runtime_transaction::transaction_meta::StaticMeta;
use solana_svm_transaction::svm_message::SVMStaticMessage;
use solana_transaction::sanitized::MessageHash;

use crate::performance_metrics::PerformanceMetrics;
use crate::priority_id::PriorityId;
use crate::priority_list::PriorityList;
use crate::transaction_map::TransactionMap;
mod priority_id;

// Re-export ExecutionStats, PriorityEntry, PriorityBundleEntry, and PerformanceSnapshot for public API
pub use crate::performance_metrics::PerformanceSnapshot;
pub use crate::priority_list::{ExecutionStats, PriorityBundleEntry, PriorityEntry};

const UNCHECKED_CAPACITY: usize = 64 * 1024;
const CHECKED_CAPACITY: usize = 64 * 1024;
const STATE_CAPACITY: usize = UNCHECKED_CAPACITY + CHECKED_CAPACITY;

const TX_REGION_SIZE: usize = std::mem::size_of::<SharableTransactionRegion>();
const TX_BATCH_PER_MESSAGE: usize = TX_REGION_SIZE + std::mem::size_of::<PriorityId>();
const TX_BATCH_SIZE: usize = TX_BATCH_PER_MESSAGE * MAX_TRANSACTIONS_PER_MESSAGE;
const_assert!(TX_BATCH_SIZE < 4096);
const TX_BATCH_META_OFFSET: usize = TX_REGION_SIZE * MAX_TRANSACTIONS_PER_MESSAGE;

/// How many percentage points before the end should we aim to fill the block.
const BLOCK_FILL_CUTOFF: u8 = 20;

/// Structure to track pending transactions for a bundle
#[derive(Debug, Clone)]
struct PendingBundle {
    /// Expected transaction hashes in this bundle
    expected_tx_hashes: Vec<Vec<u8>>,
    /// Transactions that have arrived (hash -> PriorityId)
    received_transactions: HashMap<Vec<u8>, PriorityId>,
}

impl PendingBundle {
    fn new(expected_tx_hashes: Vec<Vec<u8>>) -> Self {
        Self { expected_tx_hashes, received_transactions: HashMap::new() }
    }

    /// Check if this bundle is complete
    fn is_complete(&self) -> bool {
        self.received_transactions.len() == self.expected_tx_hashes.len()
    }

    /// Add a transaction to this pending bundle
    fn add_transaction(&mut self, tx_hash: Vec<u8>, priority_id: PriorityId) -> bool {
        if self.expected_tx_hashes.contains(&tx_hash) {
            self.received_transactions.insert(tx_hash, priority_id);
            true
        } else {
            false
        }
    }

    /// Get all received transactions
    fn get_transactions(&self) -> Vec<PriorityId> {
        self.received_transactions.values().copied().collect()
    }
}

pub struct GreedyQueues {
    tpu_to_pack: shaq::Consumer<TpuToPackMessage>,
    progress_tracker: shaq::Consumer<ProgressMessage>,
    workers: Vec<ClientWorkerSession>,
}

struct RuntimeState {
    feature_set: FeatureSet,
    fee_features: FeeFeatures,
    lamports_per_signature: u64,
    burn_percent: u64,
}

pub struct GreedyScheduler {
    allocator: Allocator,

    progress: ProgressMessage,
    runtime: RuntimeState,
    unchecked: MinMaxHeap<PriorityId>,
    checked: MinMaxHeap<PriorityId>,
    state: TransactionMap,
    in_flight_cost: u32,
    schedule_locks: HashMap<Pubkey, bool>,
    priority_list: PriorityList,

    // Bundle tracking
    /// Maps bundle signer pubkey -> PendingBundle
    pending_bundle_signers: HashMap<Pubkey, PendingBundle>,
    /// Maps bundle program pubkey -> PendingBundle
    pending_bundle_programs: HashMap<Pubkey, PendingBundle>,

    // Performance metrics
    metrics: PerformanceMetrics,
}

impl GreedyScheduler {
    #[must_use]
    pub fn new(
        ClientSession { mut allocators, tpu_to_pack, progress_tracker, workers }: ClientSession,
    ) -> (Self, GreedyQueues) {
        assert_eq!(allocators.len(), 1, "invalid number of allocators");

        let priority_list = PriorityList::with_capacity(256);
        (
            Self {
                allocator: allocators.remove(0),

                progress: ProgressMessage {
                    leader_state: 0,
                    current_slot: 0,
                    next_leader_slot: u64::MAX,
                    leader_range_end: u64::MAX,
                    remaining_cost_units: 0,
                    current_slot_progress: 0,
                },
                runtime: RuntimeState {
                    feature_set: FeatureSet::all_enabled(),
                    fee_features: FeeFeatures { enable_secp256r1_precompile: true },
                    lamports_per_signature: 5000,
                    burn_percent: 50,
                },
                unchecked: MinMaxHeap::with_capacity(UNCHECKED_CAPACITY),
                checked: MinMaxHeap::with_capacity(UNCHECKED_CAPACITY),
                state: TransactionMap::with_capacity(STATE_CAPACITY),
                in_flight_cost: 0,
                schedule_locks: HashMap::default(),
                priority_list,
                pending_bundle_signers: HashMap::new(),
                pending_bundle_programs: HashMap::new(),
                metrics: PerformanceMetrics::new(),
            },
            GreedyQueues { tpu_to_pack, progress_tracker, workers },
        )
    }

    /// Adds a signer to the priority list with unlimited quota
    pub fn add_priority_signer(&mut self, pubkey: Pubkey) {
        self.priority_list.add_priority_signer(pubkey);
    }

    /// Adds a signer to the priority list with specified quota
    pub fn add_priority_signer_with_quota(&mut self, pubkey: Pubkey, quota: u64) {
        self.priority_list
            .add_priority_signer_with_quota(pubkey, quota);
    }

    /// Removes a signer from the priority list
    pub fn remove_priority_signer(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_list.remove_priority_signer(pubkey)
    }

    /// Adds a program ID to the priority list with unlimited quota
    pub fn add_priority_program(&mut self, pubkey: Pubkey) {
        self.priority_list.add_priority_program(pubkey);
    }

    /// Adds a program ID to the priority list with specified quota
    pub fn add_priority_program_with_quota(&mut self, pubkey: Pubkey, quota: u64) {
        self.priority_list
            .add_priority_program_with_quota(pubkey, quota);
    }

    /// Removes a program ID from the priority list
    pub fn remove_priority_program(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_list.remove_priority_program(pubkey)
    }

    /// Gets signer quota information
    pub fn get_signer_quota(&self, pubkey: &Pubkey) -> Option<crate::priority_list::PriorityEntry> {
        self.priority_list.get_signer_quota(pubkey)
    }

    /// Gets program quota information
    pub fn get_program_quota(
        &self,
        pubkey: &Pubkey,
    ) -> Option<crate::priority_list::PriorityEntry> {
        self.priority_list.get_program_quota(pubkey)
    }

    /// Sets the priority multiplier
    pub fn set_priority_multiplier(&mut self, multiplier: u64) {
        self.priority_list.set_multiplier(multiplier);
    }

    /// Clears all priority lists
    pub fn clear_priority_lists(&mut self) {
        self.priority_list.clear();
    }

    /// Gets execution statistics for a priority signer
    pub fn get_signer_stats(
        &self,
        pubkey: &Pubkey,
    ) -> Option<crate::priority_list::ExecutionStats> {
        self.priority_list.get_signer_stats(pubkey)
    }

    /// Gets execution statistics for a priority program
    pub fn get_program_stats(
        &self,
        pubkey: &Pubkey,
    ) -> Option<crate::priority_list::ExecutionStats> {
        self.priority_list.get_program_stats(pubkey)
    }

    /// Gets all signer statistics
    pub fn get_all_signer_stats(
        &self,
    ) -> &hashbrown::HashMap<Pubkey, crate::priority_list::ExecutionStats> {
        self.priority_list.get_all_signer_stats()
    }

    /// Gets all program statistics
    pub fn get_all_program_stats(
        &self,
    ) -> &hashbrown::HashMap<Pubkey, crate::priority_list::ExecutionStats> {
        self.priority_list.get_all_program_stats()
    }

    /// Clears execution statistics (keeps priority lists intact)
    pub fn clear_stats(&mut self) {
        self.priority_list.clear_stats();
    }

    /// Increments signer quota by specified amount
    /// Returns true if signer was found, false otherwise
    pub fn increment_signer_quota(&mut self, pubkey: &Pubkey, amount: u64) -> bool {
        self.priority_list.increment_signer_quota(pubkey, amount)
    }

    /// Increments program quota by specified amount
    /// Returns true if program was found, false otherwise
    pub fn increment_program_quota(&mut self, pubkey: &Pubkey, amount: u64) -> bool {
        self.priority_list.increment_program_quota(pubkey, amount)
    }

    // === Bundle Signer Methods ===

    /// Adds a bundle signer with tip and transaction hashes
    pub fn add_priority_bundle_signer(
        &mut self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) {
        self.priority_list
            .add_priority_bundle_signer(pubkey, tip, tx_hashes);
    }

    /// Removes a bundle signer
    pub fn remove_priority_bundle_signer(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_list.remove_priority_bundle_signer(pubkey)
    }

    /// Updates bundle signer tip
    pub fn update_bundle_signer_tip(&mut self, pubkey: &Pubkey, tip: u64) -> bool {
        self.priority_list.update_bundle_signer_tip(pubkey, tip)
    }

    /// Adds transaction hashes to bundle signer
    pub fn add_bundle_signer_tx_hashes(
        &mut self,
        pubkey: &Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> bool {
        self.priority_list
            .add_bundle_signer_tx_hashes(pubkey, tx_hashes)
    }

    /// Gets bundle signer entry
    pub fn get_bundle_signer(&self, pubkey: &Pubkey) -> Option<&PriorityBundleEntry> {
        self.priority_list.get_bundle_signer(pubkey)
    }

    /// Gets all bundle signers
    pub fn get_all_bundle_signers(&self) -> &hashbrown::HashMap<Pubkey, PriorityBundleEntry> {
        self.priority_list.get_all_bundle_signers()
    }

    // === Bundle Program Methods ===

    /// Adds a bundle program with tip and transaction hashes
    pub fn add_priority_bundle_program(
        &mut self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) {
        self.priority_list
            .add_priority_bundle_program(pubkey, tip, tx_hashes);
    }

    /// Removes a bundle program
    pub fn remove_priority_bundle_program(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_list.remove_priority_bundle_program(pubkey)
    }

    /// Updates bundle program tip
    pub fn update_bundle_program_tip(&mut self, pubkey: &Pubkey, tip: u64) -> bool {
        self.priority_list.update_bundle_program_tip(pubkey, tip)
    }

    /// Adds transaction hashes to bundle program
    pub fn add_bundle_program_tx_hashes(
        &mut self,
        pubkey: &Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> bool {
        self.priority_list
            .add_bundle_program_tx_hashes(pubkey, tx_hashes)
    }

    /// Gets bundle program entry
    pub fn get_bundle_program(&self, pubkey: &Pubkey) -> Option<&PriorityBundleEntry> {
        self.priority_list.get_bundle_program(pubkey)
    }

    /// Gets all bundle programs
    pub fn get_all_bundle_programs(&self) -> &hashbrown::HashMap<Pubkey, PriorityBundleEntry> {
        self.priority_list.get_all_bundle_programs()
    }

    /// Clears all bundles
    pub fn clear_bundles(&mut self) {
        self.priority_list.clear_bundles();
        self.pending_bundle_signers.clear();
        self.pending_bundle_programs.clear();
    }

    /// Synchronizes pending bundles with priority list bundles
    fn sync_pending_bundles(&mut self) {
        // Sync signer bundles
        let all_bundle_signers = self.priority_list.get_all_bundle_signers();
        for (pubkey, bundle_entry) in all_bundle_signers {
            if !self.pending_bundle_signers.contains_key(pubkey) {
                self.pending_bundle_signers
                    .insert(*pubkey, PendingBundle::new(bundle_entry.tx_hashes.clone()));
            }
        }

        // Sync program bundles
        let all_bundle_programs = self.priority_list.get_all_bundle_programs();
        for (pubkey, bundle_entry) in all_bundle_programs {
            if !self.pending_bundle_programs.contains_key(pubkey) {
                self.pending_bundle_programs
                    .insert(*pubkey, PendingBundle::new(bundle_entry.tx_hashes.clone()));
            }
        }

        // Remove bundles that no longer exist in priority_list
        self.pending_bundle_signers
            .retain(|k, _| all_bundle_signers.contains_key(k));
        self.pending_bundle_programs
            .retain(|k, _| all_bundle_programs.contains_key(k));
    }

    /// Gets a snapshot of current performance metrics
    pub fn get_performance_metrics(&self) -> PerformanceSnapshot {
        PerformanceSnapshot::from(&self.metrics)
    }

    /// Check if a transaction belongs to any bundle and handle accordingly
    /// Returns Some(Vec<PriorityId>) with transactions to add to unchecked queue
    /// Returns None if transaction should wait in pending bundle
    fn try_add_to_bundle(
        &mut self,
        tx_signature: &[u8; 32],
        priority_signers: &[Pubkey],
        priority_programs: &[Pubkey],
        priority_id: PriorityId,
    ) -> Option<Vec<PriorityId>> {
        let tx_sig_vec = tx_signature.to_vec();

        println!("Tx Signature (first 32 bytes): {}", bs58::encode(tx_signature).into_string());

        // Check signer bundles
        for signer in priority_signers {
            if let Some(pending_bundle) = self.pending_bundle_signers.get_mut(signer) {
                if pending_bundle.add_transaction(tx_sig_vec.clone(), priority_id) {
                    if pending_bundle.is_complete() {
                        // Bundle is complete! Return all transactions
                        println!("Bundle is complete for priority_signers!");
                        return Some(pending_bundle.get_transactions());
                    } else {
                        // Transaction added to pending bundle, but not complete yet
                        return None;
                    }
                }
            }
        }

        // Check program bundles
        for program in priority_programs {
            if let Some(pending_bundle) = self.pending_bundle_programs.get_mut(program) {
                if pending_bundle.add_transaction(tx_sig_vec.clone(), priority_id) {
                    if pending_bundle.is_complete() {
                        // Bundle is complete! Return all transactions
                        println!("Bundle is complete for priority_programs!");
                        return Some(pending_bundle.get_transactions());
                    } else {
                        // Transaction added to pending bundle, but not complete yet
                        return None;
                    }
                }
            }
        }

        println!("Not part of any bundle - return the single transaction!");
        // Not part of any bundle - return the single transaction
        Some(vec![priority_id])
    }

    /// Resets performance metrics
    pub fn reset_performance_metrics(&mut self) {
        self.metrics.reset();
    }

    pub fn poll(&mut self, queues: &mut GreedyQueues) {
        // Drain the progress tracker so we know which slot we're on.
        self.drain_progress(queues);
        let is_leader = self.progress.leader_state == IS_LEADER;

        // Drain responses from workers.
        self.drain_worker_responses(queues);

        // Ingest a bounded amount of new transactions.
        match is_leader {
            true => self.drain_tpu(queues, 128),
            false => self.drain_tpu(queues, 1024),
        }

        // Drain pending checks.
        self.schedule_checks(queues);

        // Schedule if we're currently the leader.
        if is_leader {
            self.schedule_execute(queues);
        }

        // TODO: Think about re-checking all TXs on slot roll (or at least
        // expired TXs). If we do this we should use a dense slotmap to make
        // iteration fast.
    }

    fn drain_progress(&mut self, queues: &mut GreedyQueues) {
        queues.progress_tracker.sync();
        while let Some(msg) = queues.progress_tracker.try_read() {
            self.progress = *msg;
        }
        queues.progress_tracker.finalize();
    }

    fn drain_worker_responses(&mut self, queues: &mut GreedyQueues) {
        for worker in &mut queues.workers {
            worker.worker_to_pack.sync();
            while let Some(msg) = worker.worker_to_pack.try_read() {
                match msg.processed_code {
                    processed_codes::MAX_WORKING_SLOT_EXCEEDED => {
                        // SAFETY
                        // - Trust Agave to not have modified/freed this pointer.
                        unsafe {
                            self.allocator
                                .free_offset(msg.responses.transaction_responses_offset);
                        }

                        continue;
                    }
                    processed_codes::PROCESSED => {}
                    _ => panic!(),
                }

                match msg.responses.tag {
                    worker_message_types::CHECK_RESPONSE => self.on_check(msg),
                    worker_message_types::EXECUTION_RESPONSE => self.on_execute(msg),
                    _ => panic!(),
                }
            }
        }
    }

    fn drain_tpu(&mut self, queues: &mut GreedyQueues, max: usize) {
        let start = std::time::Instant::now();

        // Sync pending bundles with priority list
        self.sync_pending_bundles();

        queues.tpu_to_pack.sync();

        let additional = std::cmp::min(queues.tpu_to_pack.len(), max);
        let shortfall = (self.checked.len() + additional).saturating_sub(UNCHECKED_CAPACITY);

        // NB: Technically we are evicting more than we need to because not all of
        // `additional` will parse correctly & thus have a priority.
        for _ in 0..shortfall {
            let id = self.unchecked.pop_min().unwrap();
            // SAFETY:
            // - Trust Agave to behave correctly and not free these transactions.
            // - We have not previously freed this transaction.
            unsafe { self.state.remove(&self.allocator, id.key) };
        }

        // TODO: Need to dedupe already seen transactions.

        for _ in 0..additional {
            let msg = queues.tpu_to_pack.try_read().unwrap();

            match Self::calculate_priority(&self.runtime, &self.allocator, &self.priority_list, msg)
            {
                Some((
                    view,
                    message_hash,
                    priority,
                    cost,
                    priority_flags,
                    priority_signers,
                    priority_programs,
                )) => {
                    // Clone signers and programs before inserting (since insert takes ownership)
                    let signers_for_bundle = priority_signers.clone();
                    let programs_for_bundle = priority_programs.clone();

                    let key = self.state.insert(
                        msg.transaction,
                        view,
                        priority_signers,
                        priority_programs,
                    );

                    let priority_id = PriorityId { priority, cost, key, priority_flags };

                    // Try to add to bundle or get complete bundle
                    match self.try_add_to_bundle(
                        &message_hash,
                        &signers_for_bundle,
                        &programs_for_bundle,
                        priority_id,
                    ) {
                        Some(transactions) => {
                            // Either a complete bundle or a non-bundle transaction
                            for tx in transactions {
                                self.unchecked.push(tx);
                            }
                        }
                        None => {
                            // Transaction is waiting in a pending bundle
                            // Do nothing - it will be added when the bundle is complete
                        }
                    }
                }
                // SAFETY:
                // - Trust Agave to have correctly allocated & trenferred ownership of this
                //   transaction region to us.
                None => unsafe {
                    self.allocator.free_offset(msg.transaction.offset);
                },
            }
        }

        // Record TPU ingestion metrics
        if additional > 0 {
            self.metrics
                .record_tpu_ingestion(additional, start.elapsed());
        }
    }

    fn schedule_checks(&mut self, queues: &mut GreedyQueues) {
        let worker = &mut queues.workers[0];
        worker.pack_to_worker.sync();

        // Loop until worker queue is filled or backlog is empty.
        let worker_capacity = worker.pack_to_worker.capacity();
        for _ in 0..worker_capacity {
            if self.unchecked.is_empty() {
                break;
            }

            worker
                .pack_to_worker
                .try_write(PackToWorkerMessage {
                    flags: pack_message_flags::CHECK
                        | check_flags::STATUS_CHECKS
                        | check_flags::LOAD_FEE_PAYER_BALANCE
                        | check_flags::LOAD_ADDRESS_LOOKUP_TABLES,
                    max_working_slot: self.progress.current_slot + 1,
                    batch: Self::collect_batch(&self.allocator, || {
                        self.unchecked
                            .pop_max()
                            .map(|id| (id, self.state[id.key].shared))
                    }),
                })
                .unwrap();
        }

        worker.pack_to_worker.commit();
    }

    // TODO:
    //
    // - Track read/write locks for the current batch.
    //   - Once we hit a conflict, send the batch off & return.
    // - Use workers in a round robin fashion.
    // - Do not build a batch if any worker has a non-empty queue.
    fn schedule_execute(&mut self, queues: &mut GreedyQueues) {
        // println!("schedule");
        self.schedule_locks.clear();

        debug_assert_eq!(self.progress.leader_state, IS_LEADER);
        let budget_percentage =
            std::cmp::min(self.progress.current_slot_progress + BLOCK_FILL_CUTOFF, 100);
        // TODO: Would be ideal for the scheduler protocol to tell us the max block
        // units.
        let budget_limit = MAX_BLOCK_UNITS_SIMD_0256 * u64::from(budget_percentage) / 100;
        let cost_used = MAX_BLOCK_UNITS_SIMD_0256
            .saturating_sub(self.progress.remaining_cost_units)
            + u64::from(self.in_flight_cost);
        let mut budget_remaining = budget_limit.saturating_sub(cost_used);
        for worker in &mut queues.workers[1..] {
            // println!("budget_remaining: {budget_remaining}");
            if budget_remaining == 0 || self.checked.is_empty() {
                return;
            }

            let batch = Self::collect_batch(&self.allocator, || {
                self.checked
                    .pop_max()
                    .filter(|id| {
                        // Check if we can fit the TX within our budget.
                        if u64::from(id.cost) > budget_remaining {
                            self.checked.push(*id);

                            return false;
                        }

                        // Check if this transaction's read/write locks conflict with any
                        // pre-existing read/write locks.
                        let tx = &self.state[id.key];
                        // println!("{}", tx.view.signatures()[0]);
                        if tx
                            .write_locks()
                            .any(|key| self.schedule_locks.insert(*key, true).is_some())
                            || tx.read_locks().any(|key| {
                                self.schedule_locks
                                    .insert(*key, false)
                                    .is_some_and(|writable| writable)
                            })
                        {
                            println!("tx conflicting! pushing it to the end");
                            self.checked.push(*id);
                            budget_remaining = 0;

                            return false;
                        }

                        // Update the budget as we are scheduling this TX.
                        budget_remaining = budget_remaining.saturating_sub(u64::from(id.cost));

                        true
                    })
                    .map(|id| {
                        self.in_flight_cost += id.cost;

                        (id, self.state[id.key].shared)
                    })
            });

            // If we failed to schedule anything, don't send the batch.
            if batch.num_transactions == 0 {
                return;
            }

            worker.pack_to_worker.sync();
            // TODO: Figure out back pressure with workers to ensure they are all keeping
            // up.
            worker
                .pack_to_worker
                .try_write(PackToWorkerMessage {
                    flags: pack_message_flags::EXECUTE,
                    max_working_slot: self.progress.current_slot + 1,
                    batch,
                })
                .unwrap();
            worker.pack_to_worker.commit();
        }
    }

    fn on_check(&mut self, msg: &WorkerToPackMessage) {
        // SAFETY:
        // - Trust Agave to have allocated the batch/responses properly & told us the
        //   correct size.
        // - Use the correct wrapper type (check response ptr).
        // - Don't duplicate wrappers so we cannot double free.
        let (batch, responses) = unsafe {
            (
                TransactionPtrBatch::<PriorityId>::from_sharable_transaction_batch_region(
                    &msg.batch,
                    &self.allocator,
                ),
                CheckResponsesPtr::from_transaction_response_region(
                    &msg.responses,
                    &self.allocator,
                ),
            )
        };
        assert_eq!(batch.len(), responses.len());

        for ((_, id), rep) in batch.iter().zip(responses.iter().copied()) {
            let parsing_failed =
                rep.parsing_and_sanitization_flags == parsing_and_sanitization_flags::FAILED;
            let status_failed = rep.status_check_flags
                & !(status_check_flags::REQUESTED | status_check_flags::PERFORMED)
                != 0;
            if parsing_failed || status_failed {
                // SAFETY:
                // - TX was previously allocated with this allocator.
                // - Trust Agave to not free this TX while returning it to us.
                unsafe {
                    self.state.remove(&self.allocator, id.key);
                }

                continue;
            }

            // Sanity check the flags.
            assert_ne!(rep.status_check_flags & status_check_flags::REQUESTED, 0);
            assert_ne!(rep.status_check_flags & status_check_flags::PERFORMED, 0);
            assert_eq!(rep.resolve_flags, resolve_flags::REQUESTED | resolve_flags::PERFORMED);
            assert_eq!(
                rep.fee_payer_balance_flags,
                fee_payer_balance_flags::REQUESTED | fee_payer_balance_flags::PERFORMED
            );

            // Evict lowest priority if at capacity.
            if self.checked.len() == CHECKED_CAPACITY {
                let evicted = self.checked.pop_min().unwrap();
                // SAFETY
                // - We have not previously freed the underlying transaction/pubkey objects.
                unsafe { self.state.remove(&self.allocator, evicted.key) };
            }

            // Insert the new transaction (yes this may be lower priority then what
            // we just evicted but that's fine).
            self.checked.push(id);

            // Update the state to include the resolved pubkeys.
            //
            // SAFETY
            // - Trust Agave to have allocated the pubkeys properly & transferred ownership
            //   to us.
            if rep.resolved_pubkeys.num_pubkeys > 0 {
                self.state[id.key].resolved = Some(unsafe {
                    PubkeysPtr::from_sharable_pubkeys(&rep.resolved_pubkeys, &self.allocator)
                });
            }
        }

        // Free both containers.
        batch.free();
        responses.free();
    }

    fn on_execute(&mut self, msg: &WorkerToPackMessage) {
        // SAFETY:
        // - Trust Agave to have allocated the batch/responses properly & told us the
        //   correct size.
        // - Don't duplicate wrapper so we cannot double free.
        let batch: TransactionPtrBatch<PriorityId> = unsafe {
            TransactionPtrBatch::from_sharable_transaction_batch_region(&msg.batch, &self.allocator)
        };
        let responses: ExecutionResponsesPtr = unsafe {
            ExecutionResponsesPtr::from_transaction_response_region(&msg.responses, &self.allocator)
        };

        // Update statistics for priority transactions and remove in-flight costs
        let mut index = 0;
        let responses: Vec<&ExecutionResponse> = responses.iter().collect();
        assert_eq!(batch.len(), responses.len());
        for (tx, id) in batch.iter() {
            self.in_flight_cost -= id.cost;
            let response = responses[index];
            // Update statistics and quotas if this transaction had priority signers or programs
            if response.not_included_reason == not_included_reasons::NONE
                && (id.has_priority_signer() || id.has_priority_program())
            {
                let tx_state = &self.state[id.key];

                // Record stats and decrement quotas for priority signers
                for signer in &tx_state.priority_signers {
                    self.priority_list.record_signer_success(signer);
                    self.priority_list.decrement_signer_quota(signer);
                }

                // Record stats and decrement quotas for priority programs
                for program in &tx_state.priority_programs {
                    self.priority_list.record_program_success(program);
                    self.priority_list.decrement_program_quota(program);
                }
            }
            if response.not_included_reason == not_included_reasons::NONE
                && (id.has_priority_bundle_signer() || id.has_priority_bundle_program())
            {
                let tx_state = &self.state[id.key];

                // Record stats and remove for priority bundle signers
                for signer in &tx_state.priority_signers {
                    self.priority_list.record_signer_success(signer);
                    self.priority_list.remove_priority_bundle_signer(signer);
                }

                // Record stats and remove priority bundle programs
                for program in &tx_state.priority_programs {
                    self.priority_list.record_program_success(program);
                    self.priority_list.remove_priority_bundle_program(program);
                }
            }
            index += 1;

            // SAFETY:
            // - Trust Agave not to have already freed this transaction as we are the owner.
            unsafe { tx.free(&self.allocator) };
        }

        // Free the containers.
        batch.free();
        // SAFETY:
        // - Trust Agave to have allocated these responses properly.
        unsafe {
            self.allocator
                .free_offset(msg.responses.transaction_responses_offset);
        }
    }

    fn collect_batch(
        allocator: &Allocator,
        mut pop: impl FnMut() -> Option<(PriorityId, SharableTransactionRegion)>,
    ) -> SharableTransactionBatchRegion {
        // Allocate a batch that can hold all our transaction pointers.
        let transactions = allocator.allocate(TX_BATCH_SIZE as u32).unwrap();
        let transactions_offset = unsafe { allocator.offset(transactions) };

        // Get our two pointers to the TX region & meta region.
        let tx_ptr = allocator
            .ptr_from_offset(transactions_offset)
            .cast::<SharableTransactionRegion>();
        // SAFETY:
        // - Pointer is guaranteed to not overrun the allocation as we just created it
        //   with a sufficient size.
        let meta_ptr = unsafe {
            allocator
                .ptr_from_offset(transactions_offset)
                .byte_add(TX_BATCH_META_OFFSET)
                .cast::<PriorityId>()
        };

        // Fill in the batch with transaction pointers.
        let mut num_transactions = 0;
        while num_transactions < MAX_TRANSACTIONS_PER_MESSAGE {
            let Some((id, tx)) = pop() else {
                break;
            };

            // SAFETY:
            // - We have allocated the transaction batch to support at least
            //   `MAX_TRANSACTIONS_PER_MESSAGE`, we terminate the loop before we overrun the
            //   region.
            unsafe {
                tx_ptr.add(num_transactions).write(tx);
                meta_ptr.add(num_transactions).write(id);
            };

            num_transactions += 1;
        }

        SharableTransactionBatchRegion {
            num_transactions: num_transactions.try_into().unwrap(),
            transactions_offset,
        }
    }

    fn calculate_priority(
        runtime: &RuntimeState,
        allocator: &Allocator,
        priority_list: &PriorityList,
        msg: &TpuToPackMessage,
    ) -> Option<(
        TransactionView<true, TransactionPtr>,
        [u8; 32],
        u64,
        u32,
        u8,
        Vec<Pubkey>,
        Vec<Pubkey>,
    )> {
        let tx_view = SanitizedTransactionView::try_new_sanitized(
            // SAFETY:
            // - Trust Agave to have allocated the shared transactin region correctly.
            // - `SanitizedTransactionView` does not free the allocation on drop.
            unsafe {
                TransactionPtr::from_sharable_transaction_region(&msg.transaction, allocator)
            },
            true,
        )
        .ok()?;

        let tx = RuntimeTransaction::<TransactionView<true, TransactionPtr>>::try_from(
            tx_view,
            MessageHash::Compute,
            None,
        )
        .ok()?;

        // Compute transaction cost.
        let compute_budget_limits =
            compute_budget_instruction_details::ComputeBudgetInstructionDetails::try_from(
                tx.program_instructions_iter(),
            )
            .ok()?
            .sanitize_and_convert_to_compute_budget_limits(&runtime.feature_set)
            .ok()?;
        let fee_budget_limits = FeeBudgetLimits::from(compute_budget_limits);
        let cost = CostModel::calculate_cost(&tx, &runtime.feature_set).sum();

        // Compute transaction reward.
        let fee_details = solana_fee::calculate_fee_details(
            &tx,
            false,
            runtime.lamports_per_signature,
            fee_budget_limits.prioritization_fee,
            runtime.fee_features,
        );
        let burn = fee_details
            .transaction_fee()
            .checked_mul(runtime.burn_percent)?
            / 100;
        let base_fee = fee_details.transaction_fee() - burn;
        let reward = base_fee.saturating_add(fee_details.prioritization_fee());

        // Compute base priority.
        let mut priority = reward
            .saturating_mul(1_000_000)
            .saturating_div(cost.saturating_add(1));

        // Apply priority boost if transaction contains priority signers or program IDs
        // Collect priority program IDs before consuming tx
        let priority_programs: Vec<Pubkey> = tx
            .program_instructions_iter()
            .filter_map(|(program_id, _)| {
                if priority_list.is_priority_program(program_id) { Some(*program_id) } else { None }
            })
            .collect();

        let priority_bundle_programs: Vec<Pubkey> = tx
            .program_instructions_iter()
            .filter_map(|(program_id, _)| {
                if priority_list.is_priority_bundle_program(program_id) {
                    Some(*program_id)
                } else {
                    None
                }
            })
            .collect();

        // Get the transaction signature (first signature) - this is the proper identifier
        let inner_tx = tx.into_inner_transaction();
        let tx_signature = if let Some(sig) = inner_tx.signatures().first() {
            sig.as_ref().to_vec()
        } else {
            // Fallback to empty if no signature (shouldn't happen)
            vec![0u8; 64]
        };

        println!("TX Signature: {}", bs58::encode(&tx_signature).into_string());

        // Collect priority signers
        let priority_signers: Vec<Pubkey> = inner_tx
            .static_account_keys()
            .iter()
            .take(inner_tx.num_signatures() as usize)
            .filter(|key| {
                priority_list.is_priority_signer(key)
                    || priority_list.is_priority_bundle_signer(key)
            })
            .copied()
            .collect();

        let priority_bundle_signers: Vec<Pubkey> = inner_tx
            .static_account_keys()
            .iter()
            .take(inner_tx.num_signatures() as usize)
            .filter(|key| priority_list.is_priority_bundle_signer(key))
            .copied()
            .collect();

        // Build priority flags
        let mut priority_flags = 0u8;
        if !priority_signers.is_empty() {
            priority_flags |= PriorityId::FLAG_HAS_PRIORITY_SIGNER;
        }
        if !priority_programs.is_empty() {
            priority_flags |= PriorityId::FLAG_HAS_PRIORITY_PROGRAM;
        }

        if !priority_bundle_signers.is_empty() {
            priority_flags |= PriorityId::FLAG_HAS_PRIORITY_BUNDLE_SIGNER;
        }

        if !priority_bundle_programs.is_empty() {
            priority_flags |= PriorityId::FLAG_HAS_PRIORITY_BUNDLE_PROGRAM;
        }

        let is_priority_tx = !priority_signers.is_empty()
            || !priority_programs.is_empty()
            || !priority_bundle_signers.is_empty()
            || !priority_bundle_signers.is_empty()
            || !priority_bundle_programs.is_empty();

        if is_priority_tx {
            priority = priority.saturating_mul(priority_list.get_multiplier());
        }

        // Convert signature to [u8; 32] for compatibility (using first 32 bytes)
        let mut sig_bytes = [0u8; 32];
        sig_bytes.copy_from_slice(&tx_signature[..32]);

        // Return transaction with final priority, flags, and tracked pubkeys
        Some((
            inner_tx,
            sig_bytes, // Using first 32 bytes of signature instead of message_hash
            priority,
            // TODO: Is it possible to craft a TX that passes sanitization with a cost > u32::MAX?
            cost.try_into().unwrap(),
            priority_flags,
            priority_signers,
            priority_programs,
        ))
    }
}

#[cfg(test)]
mod tests {
    use std::hash::DefaultHasher;
    use std::os::fd::IntoRawFd;
    use std::ptr::NonNull;

    use agave_scheduler_bindings::worker_message_types::CheckResponse;
    use agave_scheduler_bindings::{
        IS_NOT_LEADER, SharablePubkeys, SharableTransactionRegion, WorkerToPackMessage,
        processed_codes,
    };
    use agave_scheduling_utils::handshake::server::AgaveSession;
    use agave_scheduling_utils::handshake::{self, ClientLogon};
    use agave_scheduling_utils::responses_region::allocate_check_response_region;
    use solana_compute_budget_interface::ComputeBudgetInstruction;
    use solana_hash::Hash;
    use solana_keypair::{Keypair, Signer};
    use solana_message::{AddressLookupTableAccount, v0};
    use solana_transaction::versioned::VersionedTransaction;
    use solana_transaction::{AccountMeta, Instruction, Transaction, VersionedMessage};

    use super::*;

    const MOCK_PROGRESS: ProgressMessage = ProgressMessage {
        leader_state: IS_NOT_LEADER,
        current_slot: 10,
        next_leader_slot: 11,
        leader_range_end: 11,
        remaining_cost_units: 50_000_000,
        current_slot_progress: 25,
    };

    #[test]
    fn check_no_schedule() {
        let mut harness = Harness::setup();

        // Ingest a simple transfer.
        let from = Keypair::new();
        let to = Pubkey::new_unique();

        let tx = solana_system_transaction::transfer(&from, &to, 1, Hash::new_unique());

        let mut hasher = DefaultHasher::new();
        tx.message().hash();
        harness.send_tx(&tx.into());

        // harness.send_tx(
        //     &solana_system_transaction::transfer(&from, &to, 1, Hash::new_unique()).into(),
        // );

        // Poll the greedy scheduler.
        harness.poll_scheduler();

        // Assert - A single request (to check the TX) is sent.
        let mut worker_requests = harness.drain_pack_to_workers();
        assert_eq!(worker_requests.len(), 1);
        let (worker_index, message) = worker_requests.remove(0);
        assert_eq!(message.flags & 1, pack_message_flags::CHECK);

        // Respond with OK.
        harness.send_check_ok(worker_index, message, None);
        harness.poll_scheduler();

        // Assert - Scheduler does not schedule the valid TX as we are not leader.
        assert!(harness.session.workers.iter_mut().all(|worker| {
            worker.pack_to_worker.sync();

            worker.pack_to_worker.is_empty()
        }));
    }

    #[test]
    fn check_then_schedule() {
        let mut harness = Harness::setup();

        // Notify the scheduler that node is now leader.
        harness.send_progress(ProgressMessage { leader_state: IS_LEADER, ..MOCK_PROGRESS });

        // Ingest a simple transfer.
        let from = Keypair::new();
        let to = Pubkey::new_unique();
        harness.send_tx(
            &solana_system_transaction::transfer(&from, &to, 1, Hash::new_unique()).into(),
        );

        // Poll the greedy scheduler.
        harness.poll_scheduler();

        // Assert - A single request (to check the TX) is sent.
        let mut worker_requests = harness.drain_pack_to_workers();
        assert_eq!(worker_requests.len(), 1);
        let (worker_index, message) = worker_requests.remove(0);
        assert_eq!(message.flags & 1, pack_message_flags::CHECK);

        // Respond with OK.
        harness.send_check_ok(worker_index, message, None);
        harness.poll_scheduler();

        // Assert - A single request (to execute the TX) is sent.
        let mut worker_requests = harness.drain_pack_to_workers();
        assert_eq!(worker_requests.len(), 1);
        let (_, message) = worker_requests.remove(0);
        assert_eq!(message.flags & 1, pack_message_flags::EXECUTE);
        assert_eq!(message.batch.num_transactions, 1);
    }

    #[test]
    fn schedule_by_priority_static_non_conflicting() {
        let mut harness = Harness::setup();

        // Ingest a simple transfer (with low priority).
        let payer0 = Keypair::new();
        let tx0 = noop_with_budget(&payer0, 25_000, 100);
        harness.send_tx(&tx0);
        harness.poll_scheduler();
        harness.process_checks();
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Ingest a simple transfer (with high priority).
        let payer1 = Keypair::new();
        let tx1 = noop_with_budget(&payer1, 25_000, 500);
        harness.send_tx(&tx1);
        harness.poll_scheduler();
        harness.process_checks();
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Become the leader of a slot that is 50% done with a lot of remaining cost
        // units.
        harness.send_progress(ProgressMessage {
            leader_state: IS_LEADER,
            current_slot_progress: 50,
            remaining_cost_units: 50_000_000,
            ..MOCK_PROGRESS
        });

        // Assert - Scheduler has scheduled both.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex0, ex1] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex0.signatures()[0], tx1.signatures[0]);
        assert_eq!(ex1.signatures()[0], tx0.signatures[0]);
    }

    #[test]
    fn schedule_by_priority_static_conflicting() {
        let mut harness = Harness::setup();

        // Ingest a simple transfer (with low priority).
        let payer = Keypair::new();
        let tx0 = noop_with_budget(&payer, 25_000, 100);
        harness.send_tx(&tx0);
        harness.poll_scheduler();
        harness.process_checks();
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Ingest a simple transfer (with high priority).
        let tx1 = noop_with_budget(&payer, 25_000, 500);
        harness.send_tx(&tx1);
        harness.poll_scheduler();
        harness.process_checks();
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Become the leader of a slot that is 50% done with a lot of remaining cost
        // units.
        harness.send_progress(ProgressMessage {
            leader_state: IS_LEADER,
            current_slot_progress: 50,
            remaining_cost_units: 50_000_000,
            ..MOCK_PROGRESS
        });

        // Assert - Scheduler has scheduled tx1.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex0] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex0.signatures()[0], tx1.signatures[0]);

        // Assert - Scheduler has scheduled tx0.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex1] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex1.signatures()[0], tx0.signatures[0]);
    }

    #[test]
    fn schedule_by_priority_alt_non_conflicting() {
        let mut harness = Harness::setup();
        let resolved_pubkeys = Some(vec![Pubkey::new_from_array([1; 32])]);

        // Ingest a simple transfer (with low priority).
        let payer0 = Keypair::new();
        let read_lock = Pubkey::new_unique();
        let tx0 = noop_with_alt_locks(&payer0, &[], &[read_lock], 25_000, 100);
        harness.send_tx(&tx0);
        harness.poll_scheduler();
        let (worker, msg) = harness.drain_pack_to_workers()[0];
        harness.send_check_ok(worker, msg, resolved_pubkeys.clone());
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Ingest a simple transfer (with high priority).
        let payer1 = Keypair::new();
        let tx1 = noop_with_alt_locks(&payer1, &[], &[read_lock], 25_000, 500);
        harness.send_tx(&tx1);
        harness.poll_scheduler();
        let (worker, msg) = harness.drain_pack_to_workers()[0];
        harness.send_check_ok(worker, msg, resolved_pubkeys.clone());
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Become the leader of a slot that is 50% done with a lot of remaining cost
        // units.
        harness.send_progress(ProgressMessage {
            leader_state: IS_LEADER,
            current_slot_progress: 50,
            remaining_cost_units: 50_000_000,
            ..MOCK_PROGRESS
        });

        // Assert - Scheduler has scheduled both.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex0, ex1] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex0.signatures()[0], tx1.signatures[0]);
        assert_eq!(ex1.signatures()[0], tx0.signatures[0]);
    }

    #[test]
    fn schedule_by_priority_alt_conflicting() {
        let mut harness = Harness::setup();
        let resolved_pubkeys = Some(vec![Pubkey::new_from_array([1; 32])]);

        // Ingest a simple transfer (with low priority).
        let payer0 = Keypair::new();
        let write_lock = Pubkey::new_unique();
        let tx0 = noop_with_alt_locks(&payer0, &[write_lock], &[], 25_000, 100);
        harness.send_tx(&tx0);
        harness.poll_scheduler();
        let (worker, msg) = harness.drain_pack_to_workers()[0];
        harness.send_check_ok(worker, msg, resolved_pubkeys.clone());
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Ingest a simple transfer (with high priority).
        let payer1 = Keypair::new();
        let tx1 = noop_with_alt_locks(&payer1, &[write_lock], &[], 25_000, 500);
        harness.send_tx(&tx1);
        harness.poll_scheduler();
        let (worker, msg) = harness.drain_pack_to_workers()[0];
        harness.send_check_ok(worker, msg, resolved_pubkeys.clone());
        harness.poll_scheduler();
        assert!(harness.drain_pack_to_workers().is_empty());

        // Become the leader of a slot that is 50% done with a lot of remaining cost
        // units.
        harness.send_progress(ProgressMessage {
            leader_state: IS_LEADER,
            current_slot_progress: 50,
            remaining_cost_units: 50_000_000,
            ..MOCK_PROGRESS
        });

        // Assert - Scheduler has scheduled tx1.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex0] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex0.signatures()[0], tx1.signatures[0]);

        // Assert - Scheduler has scheduled tx0.
        harness.poll_scheduler();
        let batches = harness.drain_batches();
        let [(_, batch)] = &batches[..] else {
            panic!();
        };
        let [ex1] = &batch[..] else {
            panic!();
        };
        assert_eq!(ex1.signatures()[0], tx0.signatures[0]);
    }

    struct Harness {
        slot: u64,
        session: AgaveSession,
        scheduler: GreedyScheduler,
        queues: GreedyQueues,
    }

    impl Harness {
        fn setup() -> Self {
            let logon = ClientLogon {
                worker_count: 4,
                allocator_size: 16 * 1024 * 1024,
                allocator_handles: 1,
                tpu_to_pack_capacity: 1024,
                progress_tracker_capacity: 256,
                pack_to_worker_capacity: 1024,
                worker_to_pack_capacity: 1024,
                flags: 0,
            };
            let (agave_session, files) = handshake::server::Server::setup_session(logon).unwrap();
            let client_session = handshake::client::setup_session(
                &logon,
                files.into_iter().map(IntoRawFd::into_raw_fd).collect(),
            )
            .unwrap();
            let (scheduler, queues) = GreedyScheduler::new(client_session);

            Self { slot: 0, session: agave_session, scheduler, queues }
        }

        fn allocator(&self) -> &Allocator {
            &self.session.tpu_to_pack.allocator
        }

        fn poll_scheduler(&mut self) {
            self.scheduler.poll(&mut self.queues);
        }

        fn process_checks(&mut self) {
            for (worker_index, message) in self.drain_pack_to_workers() {
                assert_eq!(message.flags & 1, pack_message_flags::CHECK);
                self.send_check_ok(worker_index, message, None);
            }
        }

        fn drain_pack_to_workers(&mut self) -> Vec<(usize, PackToWorkerMessage)> {
            self.session
                .workers
                .iter_mut()
                .enumerate()
                .flat_map(|(i, worker)| {
                    worker.pack_to_worker.sync();

                    std::iter::from_fn(move || {
                        worker.pack_to_worker.try_read().map(|msg| (i, *msg))
                    })
                })
                .collect()
        }

        fn drain_batches(&mut self) -> Vec<(usize, Vec<TransactionView<true, TransactionPtr>>)> {
            self.drain_pack_to_workers()
                .into_iter()
                .map(|(index, msg)| {
                    let batch = unsafe {
                        TransactionPtrBatch::<PriorityId>::from_sharable_transaction_batch_region(
                            &msg.batch,
                            self.allocator(),
                        )
                    };

                    (
                        index,
                        batch
                            .iter()
                            .map(|(tx, _)| TransactionView::try_new_sanitized(tx, true).unwrap())
                            .collect(),
                    )
                })
                .collect()
        }

        fn send_progress(&mut self, progress: ProgressMessage) {
            self.slot = progress.current_slot;

            self.session.progress_tracker.sync();
            self.session.progress_tracker.try_write(progress).unwrap();
            self.session.progress_tracker.commit();
        }

        fn send_tx(&mut self, tx: &VersionedTransaction) {
            // Serialize & copy the pointer to shared memory.
            let mut packet = bincode::serialize(&tx).unwrap();
            let packet_len = packet.len().try_into().unwrap();
            let pointer = self.allocator().allocate(packet_len).unwrap();
            unsafe {
                pointer.copy_from_nonoverlapping(
                    NonNull::new(packet.as_mut_ptr()).unwrap(),
                    packet.len(),
                );
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

        fn send_check_ok(
            &mut self,
            worker_index: usize,
            msg: PackToWorkerMessage,
            pubkeys: Option<Vec<Pubkey>>,
        ) {
            assert_eq!(msg.batch.num_transactions, 1);

            let resolved_pubkeys =
                pubkeys.map_or(SharablePubkeys { offset: 0, num_pubkeys: 0 }, |keys| {
                    assert!(!keys.is_empty());
                    let pubkeys = self
                        .allocator()
                        .allocate(
                            u32::try_from(std::mem::size_of::<Pubkey>() * keys.len()).unwrap(),
                        )
                        .unwrap();
                    let offset = unsafe {
                        std::ptr::copy_nonoverlapping(
                            keys.as_ptr(),
                            pubkeys.as_ptr().cast(),
                            keys.len(),
                        );

                        self.allocator().offset(pubkeys)
                    };

                    SharablePubkeys { offset, num_pubkeys: u32::try_from(keys.len()).unwrap() }
                });

            let (mut response_ptr, responses) =
                allocate_check_response_region(self.allocator(), 1).unwrap();
            unsafe {
                *response_ptr.as_mut() = CheckResponse {
                    parsing_and_sanitization_flags: 0,
                    status_check_flags: status_check_flags::REQUESTED
                        | status_check_flags::PERFORMED,
                    fee_payer_balance_flags: fee_payer_balance_flags::REQUESTED
                        | fee_payer_balance_flags::PERFORMED,
                    resolve_flags: resolve_flags::REQUESTED | resolve_flags::PERFORMED,
                    included_slot: self.slot,
                    balance_slot: self.slot,
                    fee_payer_balance: u64::from(u32::MAX),
                    resolution_slot: self.slot,
                    min_alt_deactivation_slot: u64::MAX,
                    resolved_pubkeys,
                };
            }

            let queue = &mut self.session.workers[worker_index].worker_to_pack;
            queue.sync();
            queue
                .try_write(WorkerToPackMessage {
                    batch: msg.batch,
                    processed_code: processed_codes::PROCESSED,
                    responses,
                })
                .unwrap();
            queue.commit();
        }
    }

    fn noop_with_budget(payer: &Keypair, cu_limit: u32, cu_price: u64) -> VersionedTransaction {
        Transaction::new_signed_with_payer(
            &[
                ComputeBudgetInstruction::set_compute_unit_limit(cu_limit),
                ComputeBudgetInstruction::set_compute_unit_price(cu_price),
            ],
            Some(&payer.pubkey()),
            &[&payer],
            Hash::new_from_array([1; 32]),
        )
        .into()
    }

    fn noop_with_alt_locks(
        payer: &Keypair,
        write: &[Pubkey],
        read: &[Pubkey],
        cu_limit: u32,
        cu_price: u64,
    ) -> VersionedTransaction {
        VersionedTransaction::try_new(
            VersionedMessage::V0(
                v0::Message::try_compile(
                    &payer.pubkey(),
                    &[
                        ComputeBudgetInstruction::set_compute_unit_limit(cu_limit),
                        ComputeBudgetInstruction::set_compute_unit_price(cu_price),
                        Instruction {
                            program_id: Pubkey::default(),
                            accounts: write
                                .iter()
                                .map(|key| (*key, true))
                                .chain(read.iter().map(|key| (*key, false)))
                                .map(|(key, is_writable)| AccountMeta {
                                    pubkey: key,
                                    is_signer: false,
                                    is_writable,
                                })
                                .collect(),
                            data: vec![],
                        },
                    ],
                    &[AddressLookupTableAccount {
                        key: Pubkey::new_unique(),
                        addresses: [write, read].concat(),
                    }],
                    Hash::new_from_array([1; 32]),
                )
                .unwrap(),
            ),
            &[payer],
        )
        .unwrap()
    }
}

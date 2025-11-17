use std::path::PathBuf;
use std::sync::mpsc::Receiver;
use std::thread::JoinHandle;
use std::time::Duration;

use agave_scheduling_utils::handshake::{ClientLogon, client as handshake_client};
use greedy_scheduler::{GreedyQueues, GreedyScheduler};
use toolbox::shutdown::Shutdown;

use greedy_scheduler::api::{QueryResponse, SchedulerCommand};

pub(crate) struct SchedulerThread {
    shutdown: Shutdown,
    scheduler: GreedyScheduler,
    queues: GreedyQueues,
    command_rx: Receiver<SchedulerCommand>,
}

impl SchedulerThread {
    pub(crate) fn spawn(
        shutdown: Shutdown,
        bindings_ipc: PathBuf,
        command_rx: Receiver<SchedulerCommand>,
    ) -> JoinHandle<()> {
        std::thread::Builder::new()
            .name("Scheduler".to_string())
            .spawn(move || {
                let session = handshake_client::connect(
                    &bindings_ipc,
                    ClientLogon {
                        worker_count: 4,
                        allocator_size: 256 * 1024 * 1024,
                        allocator_handles: 1,
                        tpu_to_pack_capacity: 2usize.pow(16),
                        progress_tracker_capacity: 1024, //1024
                        pack_to_worker_capacity: 1024,
                        worker_to_pack_capacity: 1024,
                        flags: 0,
                    },
                    Duration::from_secs(1),
                )
                .unwrap();
                let (scheduler, queues) = GreedyScheduler::new(session);

                SchedulerThread { shutdown, scheduler, queues, command_rx }.run();
            })
            .unwrap()
    }

    fn run(mut self) {
        while !self.shutdown.is_shutdown() {
            // Process commands from API
            while let Ok(cmd) = self.command_rx.try_recv() {
                self.process_command(cmd);
            }

            // Poll scheduler
            self.scheduler.poll(&mut self.queues);
        }
    }

    fn process_command(&mut self, cmd: SchedulerCommand) {
        use SchedulerCommand::*;

        match cmd {
            AddPrioritySigner { pubkey, quota } => {
                self.scheduler.add_priority_signer_with_quota(pubkey, quota);
                tracing::info!(%pubkey, quota, "Added priority signer with quota");
            }
            AddPrioritySignerUnlimited { pubkey } => {
                self.scheduler.add_priority_signer(pubkey);
                tracing::info!(%pubkey, "Added priority signer (unlimited)");
            }
            RemovePrioritySigner { pubkey } => {
                self.scheduler.remove_priority_signer(&pubkey);
                tracing::info!(%pubkey, "Removed priority signer");
            }
            AddPriorityProgram { pubkey, quota } => {
                self.scheduler
                    .add_priority_program_with_quota(pubkey, quota);
                tracing::info!(%pubkey, quota, "Added priority program with quota");
            }
            AddPriorityProgramUnlimited { pubkey } => {
                self.scheduler.add_priority_program(pubkey);
                tracing::info!(%pubkey, "Added priority program (unlimited)");
            }
            RemovePriorityProgram { pubkey } => {
                self.scheduler.remove_priority_program(&pubkey);
                tracing::info!(%pubkey, "Removed priority program");
            }
            SetPriorityMultiplier { multiplier } => {
                self.scheduler.set_priority_multiplier(multiplier);
                tracing::info!(multiplier, "Set priority multiplier");
            }
            ClearPriorityLists => {
                self.scheduler.clear_priority_lists();
                tracing::info!("Cleared all priority lists");
            }
            ClearStats => {
                self.scheduler.clear_stats();
                tracing::info!("Cleared statistics");
            }
            GetSignerStats { pubkey, response_tx } => {
                let stats = self.scheduler.get_signer_stats(&pubkey);
                let _ = response_tx.send(QueryResponse::SignerStats(stats));
                tracing::debug!(%pubkey, ?stats, "Queried signer stats");
            }
            GetProgramStats { pubkey, response_tx } => {
                let stats = self.scheduler.get_program_stats(&pubkey);
                let _ = response_tx.send(QueryResponse::ProgramStats(stats));
                tracing::debug!(%pubkey, ?stats, "Queried program stats");
            }
            GetSignerQuota { pubkey, response_tx } => {
                let quota = self.scheduler.get_signer_quota(&pubkey);
                let _ = response_tx.send(QueryResponse::SignerQuota(quota));
                tracing::debug!(%pubkey, ?quota, "Queried signer quota");
            }
            GetProgramQuota { pubkey, response_tx } => {
                let quota = self.scheduler.get_program_quota(&pubkey);
                let _ = response_tx.send(QueryResponse::ProgramQuota(quota));
                tracing::debug!(%pubkey, ?quota, "Queried program quota");
            }
            IncrementSignerQuota { pubkey, amount } => {
                let found = self.scheduler.increment_signer_quota(&pubkey, amount);
                if found {
                    tracing::info!(%pubkey, amount, "Incremented signer quota");
                } else {
                    tracing::warn!(%pubkey, amount, "Attempted to increment quota for unknown signer");
                }
            }
            IncrementProgramQuota { pubkey, amount } => {
                let found = self.scheduler.increment_program_quota(&pubkey, amount);
                if found {
                    tracing::info!(%pubkey, amount, "Incremented program quota");
                } else {
                    tracing::warn!(%pubkey, amount, "Attempted to increment quota for unknown program");
                }
            }
            AddBundleSigner { pubkey, tip, tx_hashes } => {
                self.scheduler
                    .add_priority_bundle_signer(pubkey, tip, tx_hashes.clone());
                tracing::info!(%pubkey, tip, tx_count = tx_hashes.len(), "Added bundle signer");
            }
            RemoveBundleSigner { pubkey } => {
                let removed = self.scheduler.remove_priority_bundle_signer(&pubkey);
                if removed {
                    tracing::info!(%pubkey, "Removed bundle signer");
                } else {
                    tracing::warn!(%pubkey, "Attempted to remove unknown bundle signer");
                }
            }
            UpdateBundleSignerTip { pubkey, tip } => {
                let updated = self.scheduler.update_bundle_signer_tip(&pubkey, tip);
                if updated {
                    tracing::info!(%pubkey, tip, "Updated bundle signer tip");
                } else {
                    tracing::warn!(%pubkey, "Attempted to update tip for unknown bundle signer");
                }
            }
            AddBundleSignerTxHashes { pubkey, tx_hashes } => {
                let updated = self
                    .scheduler
                    .add_bundle_signer_tx_hashes(&pubkey, tx_hashes.clone());
                if updated {
                    tracing::info!(%pubkey, tx_count = tx_hashes.len(), "Added tx hashes to bundle signer");
                } else {
                    tracing::warn!(%pubkey, "Attempted to add tx hashes to unknown bundle signer");
                }
            }
            GetBundleSigner { pubkey, response_tx } => {
                let bundle = self.scheduler.get_bundle_signer(&pubkey).cloned();
                let found = bundle.is_some();
                let _ = response_tx.send(QueryResponse::BundleSigner(bundle));
                tracing::debug!(%pubkey, found, "Queried bundle signer");
            }
            GetAllBundleSigners { response_tx } => {
                let bundles = self.scheduler.get_all_bundle_signers().clone();
                let _ = response_tx.send(QueryResponse::AllBundleSigners(bundles));
                tracing::debug!("Queried all bundle signers");
            }
            AddBundleProgram { pubkey, tip, tx_hashes } => {
                self.scheduler
                    .add_priority_bundle_program(pubkey, tip, tx_hashes.clone());
                tracing::info!(%pubkey, tip, tx_count = tx_hashes.len(), "Added bundle program");
            }
            RemoveBundleProgram { pubkey } => {
                let removed = self.scheduler.remove_priority_bundle_program(&pubkey);
                if removed {
                    tracing::info!(%pubkey, "Removed bundle program");
                } else {
                    tracing::warn!(%pubkey, "Attempted to remove unknown bundle program");
                }
            }
            UpdateBundleProgramTip { pubkey, tip } => {
                let updated = self.scheduler.update_bundle_program_tip(&pubkey, tip);
                if updated {
                    tracing::info!(%pubkey, tip, "Updated bundle program tip");
                } else {
                    tracing::warn!(%pubkey, "Attempted to update tip for unknown bundle program");
                }
            }
            AddBundleProgramTxHashes { pubkey, tx_hashes } => {
                let updated = self
                    .scheduler
                    .add_bundle_program_tx_hashes(&pubkey, tx_hashes.clone());
                if updated {
                    tracing::info!(%pubkey, tx_count = tx_hashes.len(), "Added tx hashes to bundle program");
                } else {
                    tracing::warn!(%pubkey, "Attempted to add tx hashes to unknown bundle program");
                }
            }
            GetBundleProgram { pubkey, response_tx } => {
                let bundle = self.scheduler.get_bundle_program(&pubkey).cloned();
                let found = bundle.is_some();
                let _ = response_tx.send(QueryResponse::BundleProgram(bundle));
                tracing::debug!(%pubkey, found, "Queried bundle program");
            }
            GetAllBundlePrograms { response_tx } => {
                let bundles = self.scheduler.get_all_bundle_programs().clone();
                let _ = response_tx.send(QueryResponse::AllBundlePrograms(bundles));
                tracing::debug!("Queried all bundle programs");
            }
            ClearBundles => {
                self.scheduler.clear_bundles();
                tracing::info!("Cleared all bundles");
            }
            GetAllSignerStats { response_tx } => {
                let stats = self.scheduler.get_all_signer_stats().clone();
                let _ = response_tx.send(QueryResponse::AllSignerStats(stats));
                tracing::debug!("Queried all signer stats");
            }
            GetAllProgramStats { response_tx } => {
                let stats = self.scheduler.get_all_program_stats().clone();
                let _ = response_tx.send(QueryResponse::AllProgramStats(stats));
                tracing::debug!("Queried all program stats");
            }
            GetPerformanceMetrics { response_tx } => {
                let metrics = self.scheduler.get_performance_metrics();
                let _ = response_tx.send(QueryResponse::PerformanceMetrics(metrics));
                tracing::debug!("Queried performance metrics");
            }
            ResetPerformanceMetrics => {
                self.scheduler.reset_performance_metrics();
                tracing::info!("Reset performance metrics");
            }
        }
    }
}

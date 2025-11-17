use crate::{ExecutionStats, PerformanceSnapshot, PriorityBundleEntry, PriorityEntry};
use hashbrown::HashMap;
use solana_pubkey::Pubkey;
use std::sync::mpsc::{self, Receiver, Sender};

/// Response from scheduler queries
#[derive(Debug, Clone)]
pub enum QueryResponse {
    /// Signer statistics
    SignerStats(Option<ExecutionStats>),
    /// Program statistics
    ProgramStats(Option<ExecutionStats>),
    /// Signer quota
    SignerQuota(Option<PriorityEntry>),
    /// Program quota
    ProgramQuota(Option<PriorityEntry>),
    /// All signer statistics
    AllSignerStats(HashMap<Pubkey, ExecutionStats>),
    /// All program statistics
    AllProgramStats(HashMap<Pubkey, ExecutionStats>),
    /// Performance metrics
    PerformanceMetrics(PerformanceSnapshot),
    /// Bundle signer entry
    BundleSigner(Option<PriorityBundleEntry>),
    /// Bundle program entry
    BundleProgram(Option<PriorityBundleEntry>),
    /// All bundle signers
    AllBundleSigners(HashMap<Pubkey, PriorityBundleEntry>),
    /// All bundle programs
    AllBundlePrograms(HashMap<Pubkey, PriorityBundleEntry>),
}

/// Commands that can be sent to the scheduler
#[derive(Debug, Clone)]
pub enum SchedulerCommand {
    /// Add a signer to priority list with specified quota
    AddPrioritySigner { pubkey: Pubkey, quota: u64 },
    /// Add a signer to priority list with unlimited quota
    AddPrioritySignerUnlimited { pubkey: Pubkey },
    /// Remove a signer from priority list
    RemovePrioritySigner { pubkey: Pubkey },
    /// Add a program to priority list with specified quota
    AddPriorityProgram { pubkey: Pubkey, quota: u64 },
    /// Add a program to priority list with unlimited quota
    AddPriorityProgramUnlimited { pubkey: Pubkey },
    /// Remove a program from priority list
    RemovePriorityProgram { pubkey: Pubkey },
    /// Set priority multiplier
    SetPriorityMultiplier { multiplier: u64 },
    /// Clear all priority lists
    ClearPriorityLists,
    /// Clear statistics
    ClearStats,
    /// Query signer statistics
    GetSignerStats { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Query program statistics
    GetProgramStats { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Query signer quota
    GetSignerQuota { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Query program quota
    GetProgramQuota { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Query all signer statistics
    GetAllSignerStats { response_tx: Sender<QueryResponse> },
    /// Query all program statistics
    GetAllProgramStats { response_tx: Sender<QueryResponse> },
    /// Query performance metrics
    GetPerformanceMetrics { response_tx: Sender<QueryResponse> },
    /// Reset performance metrics
    ResetPerformanceMetrics,
    /// Increment signer quota
    IncrementSignerQuota { pubkey: Pubkey, amount: u64 },
    /// Increment program quota
    IncrementProgramQuota { pubkey: Pubkey, amount: u64 },
    /// Add bundle signer (tx_hashes contains transaction signatures, first 32 bytes)
    AddBundleSigner { pubkey: Pubkey, tip: u64, tx_hashes: Vec<Vec<u8>> },
    /// Remove bundle signer
    RemoveBundleSigner { pubkey: Pubkey },
    /// Update bundle signer tip
    UpdateBundleSignerTip { pubkey: Pubkey, tip: u64 },
    /// Add transaction signatures to bundle signer (first 32 bytes of each signature)
    AddBundleSignerTxHashes { pubkey: Pubkey, tx_hashes: Vec<Vec<u8>> },
    /// Get bundle signer
    GetBundleSigner { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Get all bundle signers
    GetAllBundleSigners { response_tx: Sender<QueryResponse> },
    /// Add bundle program (tx_hashes contains transaction signatures, first 32 bytes)
    AddBundleProgram { pubkey: Pubkey, tip: u64, tx_hashes: Vec<Vec<u8>> },
    /// Remove bundle program
    RemoveBundleProgram { pubkey: Pubkey },
    /// Update bundle program tip
    UpdateBundleProgramTip { pubkey: Pubkey, tip: u64 },
    /// Add transaction signatures to bundle program (first 32 bytes of each signature)
    AddBundleProgramTxHashes { pubkey: Pubkey, tx_hashes: Vec<Vec<u8>> },
    /// Get bundle program
    GetBundleProgram { pubkey: Pubkey, response_tx: Sender<QueryResponse> },
    /// Get all bundle programs
    GetAllBundlePrograms { response_tx: Sender<QueryResponse> },
    /// Clear all bundles
    ClearBundles,
}

/// API for managing scheduler priority lists
pub struct SchedulerApi {
    tx: Sender<SchedulerCommand>,
}

impl SchedulerApi {
    /// Creates a new API instance with a command sender
    pub fn new() -> (Self, Receiver<SchedulerCommand>) {
        let (tx, rx) = mpsc::channel();
        (Self { tx }, rx)
    }

    /// Adds a signer to the priority list with specified quota
    pub fn add_priority_signer(&self, pubkey: Pubkey, quota: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddPrioritySigner { pubkey, quota })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Adds a signer to the priority list with unlimited quota
    pub fn add_priority_signer_unlimited(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddPrioritySignerUnlimited { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Removes a signer from the priority list
    pub fn remove_priority_signer(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::RemovePrioritySigner { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Adds a program to the priority list with specified quota
    pub fn add_priority_program(&self, pubkey: Pubkey, quota: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddPriorityProgram { pubkey, quota })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Adds a program to the priority list with unlimited quota
    pub fn add_priority_program_unlimited(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddPriorityProgramUnlimited { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Removes a program from the priority list
    pub fn remove_priority_program(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::RemovePriorityProgram { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Sets the priority multiplier
    pub fn set_priority_multiplier(&self, multiplier: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::SetPriorityMultiplier { multiplier })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Clears all priority lists
    pub fn clear_priority_lists(&self) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::ClearPriorityLists)
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Clears statistics
    pub fn clear_stats(&self) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::ClearStats)
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Gets signer statistics
    pub fn get_signer_stats(&self, pubkey: Pubkey) -> Result<Option<ExecutionStats>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetSignerStats { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::SignerStats(stats)) => Ok(stats),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets program statistics
    pub fn get_program_stats(&self, pubkey: Pubkey) -> Result<Option<ExecutionStats>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetProgramStats { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::ProgramStats(stats)) => Ok(stats),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets signer quota
    pub fn get_signer_quota(&self, pubkey: Pubkey) -> Result<Option<PriorityEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetSignerQuota { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::SignerQuota(quota)) => Ok(quota),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets program quota
    pub fn get_program_quota(&self, pubkey: Pubkey) -> Result<Option<PriorityEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetProgramQuota { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::ProgramQuota(quota)) => Ok(quota),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Increments signer quota by specified amount
    pub fn increment_signer_quota(&self, pubkey: Pubkey, amount: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::IncrementSignerQuota { pubkey, amount })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Increments program quota by specified amount
    pub fn increment_program_quota(&self, pubkey: Pubkey, amount: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::IncrementProgramQuota { pubkey, amount })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Gets all signer statistics
    pub fn get_all_signer_stats(&self) -> Result<HashMap<Pubkey, ExecutionStats>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetAllSignerStats { response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::AllSignerStats(stats)) => Ok(stats),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets all program statistics
    pub fn get_all_program_stats(&self) -> Result<HashMap<Pubkey, ExecutionStats>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetAllProgramStats { response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::AllProgramStats(stats)) => Ok(stats),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets performance metrics snapshot
    pub fn get_performance_metrics(&self) -> Result<PerformanceSnapshot, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetPerformanceMetrics { response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::PerformanceMetrics(metrics)) => Ok(metrics),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Resets performance metrics
    pub fn reset_performance_metrics(&self) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::ResetPerformanceMetrics)
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    // === Bundle Signer Methods ===

    /// Adds a bundle signer with tip and transaction hashes
    pub fn add_bundle_signer(
        &self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddBundleSigner { pubkey, tip, tx_hashes })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Removes a bundle signer
    pub fn remove_bundle_signer(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::RemoveBundleSigner { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Updates bundle signer tip
    pub fn update_bundle_signer_tip(&self, pubkey: Pubkey, tip: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::UpdateBundleSignerTip { pubkey, tip })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Adds transaction hashes to bundle signer
    pub fn add_bundle_signer_tx_hashes(
        &self,
        pubkey: Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddBundleSignerTxHashes { pubkey, tx_hashes })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Gets bundle signer entry
    pub fn get_bundle_signer(&self, pubkey: Pubkey) -> Result<Option<PriorityBundleEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetBundleSigner { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::BundleSigner(entry)) => Ok(entry),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets all bundle signers
    pub fn get_all_bundle_signers(&self) -> Result<HashMap<Pubkey, PriorityBundleEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetAllBundleSigners { response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::AllBundleSigners(bundles)) => Ok(bundles),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    // === Bundle Program Methods ===

    /// Adds a bundle program with tip and transaction hashes
    pub fn add_bundle_program(
        &self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddBundleProgram { pubkey, tip, tx_hashes })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Removes a bundle program
    pub fn remove_bundle_program(&self, pubkey: Pubkey) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::RemoveBundleProgram { pubkey })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Updates bundle program tip
    pub fn update_bundle_program_tip(&self, pubkey: Pubkey, tip: u64) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::UpdateBundleProgramTip { pubkey, tip })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Adds transaction hashes to bundle program
    pub fn add_bundle_program_tx_hashes(
        &self,
        pubkey: Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::AddBundleProgramTxHashes { pubkey, tx_hashes })
            .map_err(|e| format!("Failed to send command: {}", e))
    }

    /// Gets bundle program entry
    pub fn get_bundle_program(
        &self,
        pubkey: Pubkey,
    ) -> Result<Option<PriorityBundleEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetBundleProgram { pubkey, response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::BundleProgram(entry)) => Ok(entry),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Gets all bundle programs
    pub fn get_all_bundle_programs(&self) -> Result<HashMap<Pubkey, PriorityBundleEntry>, String> {
        let (response_tx, response_rx) = mpsc::channel();
        self.tx
            .send(SchedulerCommand::GetAllBundlePrograms { response_tx })
            .map_err(|e| format!("Failed to send command: {}", e))?;

        match response_rx.recv() {
            Ok(QueryResponse::AllBundlePrograms(bundles)) => Ok(bundles),
            Ok(_) => Err("Unexpected response type".to_string()),
            Err(e) => Err(format!("Failed to receive response: {}", e)),
        }
    }

    /// Clears all bundles
    pub fn clear_bundles(&self) -> Result<(), String> {
        self.tx
            .send(SchedulerCommand::ClearBundles)
            .map_err(|e| format!("Failed to send command: {}", e))
    }
}

impl Clone for SchedulerApi {
    fn clone(&self) -> Self {
        Self { tx: self.tx.clone() }
    }
}

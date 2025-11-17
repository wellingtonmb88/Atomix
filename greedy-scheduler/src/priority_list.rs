use hashbrown::HashMap;
use solana_pubkey::Pubkey;

/// Entry in priority list with quota tracking
#[derive(Debug, Clone, Copy)]
pub struct PriorityEntry {
    /// Initial quota (number of transactions allowed)
    pub initial_quota: u64,
    /// Remaining quota
    pub remaining_quota: u64,
}

/// Entry in priority bundle list with quota tracking
#[derive(Debug, Clone)]
pub struct PriorityBundleEntry {
    pub tip: u64,
    pub tx_hashes: Vec<Vec<u8>>,
}

impl PriorityEntry {
    /// Creates a new entry with specified quota
    pub fn new(quota: u64) -> Self {
        Self { initial_quota: quota, remaining_quota: quota }
    }

    /// Creates an entry with unlimited quota
    pub fn unlimited() -> Self {
        Self { initial_quota: u64::MAX, remaining_quota: u64::MAX }
    }

    /// Decrements the quota by 1, returns true if quota exhausted
    pub fn decrement(&mut self) -> bool {
        if self.remaining_quota > 0 {
            self.remaining_quota -= 1;
        }
        self.remaining_quota == 0
    }

    /// Checks if quota is exhausted
    pub fn is_exhausted(&self) -> bool {
        self.remaining_quota == 0
    }

    /// Gets usage percentage (0.0 to 1.0)
    pub fn usage_percentage(&self) -> f64 {
        if self.initial_quota == u64::MAX {
            0.0 // Unlimited quota
        } else {
            let used = self.initial_quota.saturating_sub(self.remaining_quota);
            used as f64 / self.initial_quota as f64
        }
    }
}

/// Statistics for tracking signer/program execution success
#[derive(Debug, Clone, Copy, Default)]
pub struct ExecutionStats {
    /// Total number of transactions executed
    pub total_executed: u64,
    /// Total number of successful executions
    pub successful: u64,
}

impl ExecutionStats {
    /// Records a successful execution
    pub fn record_success(&mut self) {
        self.total_executed += 1;
        self.successful += 1;
    }

    /// Records a failed execution
    pub fn record_failure(&mut self) {
        self.total_executed += 1;
    }

    /// Calculates success rate (0.0 to 1.0)
    pub fn success_rate(&self) -> f64 {
        if self.total_executed == 0 {
            0.0
        } else {
            self.successful as f64 / self.total_executed as f64
        }
    }
}

/// In-memory priority list for signers and program IDs
pub struct PriorityList {
    /// Signers that should have increased priority with quota tracking
    priority_signers: HashMap<Pubkey, PriorityEntry>,
    /// Signers that should have priority bundle
    priority_bundle_signers: HashMap<Pubkey, PriorityBundleEntry>,
    /// Program IDs that should have increased priority with quota tracking
    priority_programs: HashMap<Pubkey, PriorityEntry>,
    /// Program IDs that should have priority bundle
    priority_bundle_programs: HashMap<Pubkey, PriorityBundleEntry>,
    /// Priority multiplier for privileged transactions (default: 10x)
    priority_multiplier: u64,
    /// Statistics for priority signers
    signer_stats: HashMap<Pubkey, ExecutionStats>,
    /// Statistics for priority programs
    program_stats: HashMap<Pubkey, ExecutionStats>,
}

impl PriorityList {
    /// Creates a new empty priority list
    pub fn new() -> Self {
        Self {
            priority_signers: HashMap::new(),
            priority_bundle_signers: HashMap::new(),
            priority_programs: HashMap::new(),
            priority_bundle_programs: HashMap::new(),
            priority_multiplier: 10,
            signer_stats: HashMap::new(),
            program_stats: HashMap::new(),
        }
    }

    /// Creates a new list with initial capacity
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            priority_signers: HashMap::with_capacity(capacity),
            priority_bundle_signers: HashMap::with_capacity(capacity),
            priority_programs: HashMap::with_capacity(capacity),
            priority_bundle_programs: HashMap::with_capacity(capacity),
            priority_multiplier: 10,
            signer_stats: HashMap::with_capacity(capacity),
            program_stats: HashMap::with_capacity(capacity),
        }
    }

    /// Sets the priority multiplier
    pub fn set_multiplier(&mut self, multiplier: u64) {
        self.priority_multiplier = multiplier;
    }

    /// Adds a signer to the priority list with specified quota
    pub fn add_priority_signer_with_quota(&mut self, pubkey: Pubkey, quota: u64) {
        self.priority_signers
            .insert(pubkey, PriorityEntry::new(quota));
    }

    /// Adds a signer to the priority list with unlimited quota
    pub fn add_priority_signer(&mut self, pubkey: Pubkey) {
        self.priority_signers
            .insert(pubkey, PriorityEntry::unlimited());
    }

    /// Removes a signer from the priority list
    pub fn remove_priority_signer(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_signers.remove(pubkey).is_some()
    }

    // === Priority Bundle Signer Methods ===

    /// Adds a bundle signer with tip and transaction hashes
    pub fn add_priority_bundle_signer(
        &mut self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) {
        self.priority_bundle_signers
            .insert(pubkey, PriorityBundleEntry { tip, tx_hashes });
    }

    /// Removes a bundle signer from the priority list
    pub fn remove_priority_bundle_signer(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_bundle_signers.remove(pubkey).is_some()
    }

    /// Checks if a signer has a priority bundle
    pub fn is_priority_bundle_signer(&self, pubkey: &Pubkey) -> bool {
        self.priority_bundle_signers.contains_key(pubkey)
    }

    /// Gets priority bundle entry for a signer
    pub fn get_bundle_signer(&self, pubkey: &Pubkey) -> Option<&PriorityBundleEntry> {
        self.priority_bundle_signers.get(pubkey)
    }

    /// Updates the tip for a bundle signer
    pub fn update_bundle_signer_tip(&mut self, pubkey: &Pubkey, new_tip: u64) -> bool {
        if let Some(entry) = self.priority_bundle_signers.get_mut(pubkey) {
            entry.tip = new_tip;
            true
        } else {
            false
        }
    }

    /// Adds transaction hashes to a bundle signer
    pub fn add_bundle_signer_tx_hashes(
        &mut self,
        pubkey: &Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> bool {
        if let Some(entry) = self.priority_bundle_signers.get_mut(pubkey) {
            entry.tx_hashes.extend(tx_hashes);
            true
        } else {
            false
        }
    }

    /// Removes specific transaction hash from a bundle signer
    pub fn remove_bundle_signer_tx_hash(&mut self, pubkey: &Pubkey, tx_hash: Vec<u8>) -> bool {
        if let Some(entry) = self.priority_bundle_signers.get_mut(pubkey) {
            if let Some(pos) = entry.tx_hashes.iter().position(|h| h == &tx_hash) {
                entry.tx_hashes.remove(pos);
                return true;
            }
        }
        false
    }

    /// Returns the number of priority bundle signers
    pub fn priority_bundle_signers_count(&self) -> usize {
        self.priority_bundle_signers.len()
    }

    /// Adds a program ID to the priority list with specified quota
    pub fn add_priority_program_with_quota(&mut self, pubkey: Pubkey, quota: u64) {
        self.priority_programs
            .insert(pubkey, PriorityEntry::new(quota));
    }

    /// Adds a program ID to the priority list with unlimited quota
    pub fn add_priority_program(&mut self, pubkey: Pubkey) {
        self.priority_programs
            .insert(pubkey, PriorityEntry::unlimited());
    }

    /// Removes a program ID from the priority list
    pub fn remove_priority_program(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_programs.remove(pubkey).is_some()
    }

    // === Priority Bundle Program Methods ===

    /// Adds a bundle program with tip and transaction hashes
    pub fn add_priority_bundle_program(
        &mut self,
        pubkey: Pubkey,
        tip: u64,
        tx_hashes: Vec<Vec<u8>>,
    ) {
        self.priority_bundle_programs
            .insert(pubkey, PriorityBundleEntry { tip, tx_hashes });
    }

    /// Removes a bundle program from the priority list
    pub fn remove_priority_bundle_program(&mut self, pubkey: &Pubkey) -> bool {
        self.priority_bundle_programs.remove(pubkey).is_some()
    }

    /// Checks if a program has a priority bundle
    pub fn is_priority_bundle_program(&self, pubkey: &Pubkey) -> bool {
        self.priority_bundle_programs.contains_key(pubkey)
    }

    /// Gets priority bundle entry for a program
    pub fn get_bundle_program(&self, pubkey: &Pubkey) -> Option<&PriorityBundleEntry> {
        self.priority_bundle_programs.get(pubkey)
    }

    /// Updates the tip for a bundle program
    pub fn update_bundle_program_tip(&mut self, pubkey: &Pubkey, new_tip: u64) -> bool {
        if let Some(entry) = self.priority_bundle_programs.get_mut(pubkey) {
            entry.tip = new_tip;
            true
        } else {
            false
        }
    }

    /// Adds transaction hashes to a bundle program
    pub fn add_bundle_program_tx_hashes(
        &mut self,
        pubkey: &Pubkey,
        tx_hashes: Vec<Vec<u8>>,
    ) -> bool {
        if let Some(entry) = self.priority_bundle_programs.get_mut(pubkey) {
            entry.tx_hashes.extend(tx_hashes);
            true
        } else {
            false
        }
    }

    /// Removes specific transaction hash from a bundle program
    pub fn remove_bundle_program_tx_hash(&mut self, pubkey: &Pubkey, tx_hash: Vec<u8>) -> bool {
        if let Some(entry) = self.priority_bundle_programs.get_mut(pubkey) {
            if let Some(pos) = entry.tx_hashes.iter().position(|h| h == &tx_hash) {
                entry.tx_hashes.remove(pos);
                return true;
            }
        }
        false
    }

    /// Returns the number of priority bundle programs
    pub fn priority_bundle_programs_count(&self) -> usize {
        self.priority_bundle_programs.len()
    }

    /// Checks if a signer is in the priority list (and has remaining quota)
    pub fn is_priority_signer(&self, pubkey: &Pubkey) -> bool {
        self.priority_signers
            .get(pubkey)
            .map_or(false, |entry| !entry.is_exhausted())
    }

    /// Checks if a program ID is in the priority list (and has remaining quota)
    pub fn is_priority_program(&self, pubkey: &Pubkey) -> bool {
        self.priority_programs
            .get(pubkey)
            .map_or(false, |entry| !entry.is_exhausted())
    }

    /// Gets signer quota information
    pub fn get_signer_quota(&self, pubkey: &Pubkey) -> Option<PriorityEntry> {
        self.priority_signers.get(pubkey).copied()
    }

    /// Gets program quota information
    pub fn get_program_quota(&self, pubkey: &Pubkey) -> Option<PriorityEntry> {
        self.priority_programs.get(pubkey).copied()
    }

    /// Decrements signer quota and removes if exhausted
    pub fn decrement_signer_quota(&mut self, pubkey: &Pubkey) {
        if let Some(entry) = self.priority_signers.get_mut(pubkey) {
            if entry.decrement() {
                self.priority_signers.remove(pubkey);
            }
        }
    }

    /// Decrements program quota and removes if exhausted
    pub fn decrement_program_quota(&mut self, pubkey: &Pubkey) {
        if let Some(entry) = self.priority_programs.get_mut(pubkey) {
            if entry.decrement() {
                self.priority_programs.remove(pubkey);
            }
        }
    }

    /// Increments signer quota by specified amount
    /// Returns true if signer was found, false otherwise
    pub fn increment_signer_quota(&mut self, pubkey: &Pubkey, amount: u64) -> bool {
        if let Some(entry) = self.priority_signers.get_mut(pubkey) {
            // Handle unlimited quota case
            if entry.remaining_quota == u64::MAX {
                return true; // Already unlimited, no need to increment
            }

            // Increment remaining quota, saturating at u64::MAX
            entry.remaining_quota = entry.remaining_quota.saturating_add(amount);

            // Also update initial quota to reflect total allocation
            entry.initial_quota = entry.initial_quota.saturating_add(amount);

            true
        } else {
            false
        }
    }

    /// Increments program quota by specified amount
    /// Returns true if program was found, false otherwise
    pub fn increment_program_quota(&mut self, pubkey: &Pubkey, amount: u64) -> bool {
        if let Some(entry) = self.priority_programs.get_mut(pubkey) {
            // Handle unlimited quota case
            if entry.remaining_quota == u64::MAX {
                return true; // Already unlimited, no need to increment
            }

            // Increment remaining quota, saturating at u64::MAX
            entry.remaining_quota = entry.remaining_quota.saturating_add(amount);

            // Also update initial quota to reflect total allocation
            entry.initial_quota = entry.initial_quota.saturating_add(amount);

            true
        } else {
            false
        }
    }

    /// Gets the priority multiplier
    pub fn get_multiplier(&self) -> u64 {
        self.priority_multiplier
    }

    /// Clears all priority lists (both regular and bundle)
    pub fn clear(&mut self) {
        self.priority_signers.clear();
        self.priority_bundle_signers.clear();
        self.priority_programs.clear();
        self.priority_bundle_programs.clear();
    }

    /// Clears only bundle lists
    pub fn clear_bundles(&mut self) {
        self.priority_bundle_signers.clear();
        self.priority_bundle_programs.clear();
    }

    /// Gets all bundle signers
    pub fn get_all_bundle_signers(&self) -> &HashMap<Pubkey, PriorityBundleEntry> {
        &self.priority_bundle_signers
    }

    /// Gets all bundle programs
    pub fn get_all_bundle_programs(&self) -> &HashMap<Pubkey, PriorityBundleEntry> {
        &self.priority_bundle_programs
    }

    /// Returns the number of priority signers
    pub fn priority_signers_count(&self) -> usize {
        self.priority_signers.len()
    }

    /// Returns the number of priority program IDs
    pub fn priority_programs_count(&self) -> usize {
        self.priority_programs.len()
    }

    /// Records successful execution for signers
    pub fn record_signer_success(&mut self, pubkey: &Pubkey) {
        self.signer_stats
            .entry(*pubkey)
            .or_default()
            .record_success();
    }

    /// Records failed execution for signers
    pub fn record_signer_failure(&mut self, pubkey: &Pubkey) {
        self.signer_stats
            .entry(*pubkey)
            .or_default()
            .record_failure();
    }

    /// Records successful execution for programs
    pub fn record_program_success(&mut self, pubkey: &Pubkey) {
        self.program_stats
            .entry(*pubkey)
            .or_default()
            .record_success();
    }

    /// Records failed execution for programs
    pub fn record_program_failure(&mut self, pubkey: &Pubkey) {
        self.program_stats
            .entry(*pubkey)
            .or_default()
            .record_failure();
    }

    /// Gets statistics for a signer
    pub fn get_signer_stats(&self, pubkey: &Pubkey) -> Option<ExecutionStats> {
        self.signer_stats.get(pubkey).copied()
    }

    /// Gets statistics for a program
    pub fn get_program_stats(&self, pubkey: &Pubkey) -> Option<ExecutionStats> {
        self.program_stats.get(pubkey).copied()
    }

    /// Gets all signer statistics
    pub fn get_all_signer_stats(&self) -> &HashMap<Pubkey, ExecutionStats> {
        &self.signer_stats
    }

    /// Gets all program statistics
    pub fn get_all_program_stats(&self) -> &HashMap<Pubkey, ExecutionStats> {
        &self.program_stats
    }

    /// Clears all statistics (keeps priority lists intact)
    pub fn clear_stats(&mut self) {
        self.signer_stats.clear();
        self.program_stats.clear();
    }
}

impl Default for PriorityList {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_priority_list_signers() {
        let mut list = PriorityList::new();
        let signer = Pubkey::new_unique();

        assert!(!list.is_priority_signer(&signer));

        list.add_priority_signer(signer);
        assert!(list.is_priority_signer(&signer));
        assert_eq!(list.priority_signers_count(), 1);

        list.remove_priority_signer(&signer);
        assert!(!list.is_priority_signer(&signer));
        assert_eq!(list.priority_signers_count(), 0);
    }

    #[test]
    fn test_priority_list_programs() {
        let mut list = PriorityList::new();
        let program = Pubkey::new_unique();

        assert!(!list.is_priority_program(&program));

        list.add_priority_program(program);
        assert!(list.is_priority_program(&program));
        assert_eq!(list.priority_programs_count(), 1);

        list.remove_priority_program(&program);
        assert!(!list.is_priority_program(&program));
        assert_eq!(list.priority_programs_count(), 0);
    }

    #[test]
    fn test_priority_multiplier() {
        let mut list = PriorityList::new();
        assert_eq!(list.get_multiplier(), 10);

        list.set_multiplier(20);
        assert_eq!(list.get_multiplier(), 20);
    }

    #[test]
    fn test_clear() {
        let mut list = PriorityList::new();
        list.add_priority_signer(Pubkey::new_unique());
        list.add_priority_program(Pubkey::new_unique());

        assert_eq!(list.priority_signers_count(), 1);
        assert_eq!(list.priority_programs_count(), 1);

        list.clear();
        assert_eq!(list.priority_signers_count(), 0);
        assert_eq!(list.priority_programs_count(), 0);
    }

    #[test]
    fn test_priority_bundle_signers() {
        let mut list = PriorityList::new();
        let signer = Pubkey::new_unique();
        let tx_hashes = vec![vec![1, 2, 3], vec![4, 5, 6], vec![7, 8, 9]];

        // Test add
        list.add_priority_bundle_signer(signer, 1000, tx_hashes.clone());
        assert!(list.is_priority_bundle_signer(&signer));
        assert_eq!(list.priority_bundle_signers_count(), 1);

        // Test get
        let entry = list.get_bundle_signer(&signer).unwrap();
        assert_eq!(entry.tip, 1000);
        assert_eq!(entry.tx_hashes, tx_hashes);

        // Test update tip
        assert!(list.update_bundle_signer_tip(&signer, 2000));
        let entry = list.get_bundle_signer(&signer).unwrap();
        assert_eq!(entry.tip, 2000);

        // Test add tx hashes
        assert!(list.add_bundle_signer_tx_hashes(&signer, vec![vec![9, 9, 9]]));
        let entry = list.get_bundle_signer(&signer).unwrap();
        assert_eq!(entry.tx_hashes.len(), 4);
        assert!(entry.tx_hashes.contains(&vec![9, 9, 9]));

        // Test remove tx hash
        assert!(list.remove_bundle_signer_tx_hash(&signer, vec![4, 5, 6]));
        let entry = list.get_bundle_signer(&signer).unwrap();
        assert_eq!(entry.tx_hashes.len(), 3);
        assert!(!entry.tx_hashes.contains(&vec![4, 5, 6]));

        // Test remove
        assert!(list.remove_priority_bundle_signer(&signer));
        assert!(!list.is_priority_bundle_signer(&signer));
        assert_eq!(list.priority_bundle_signers_count(), 0);
    }

    #[test]
    fn test_priority_bundle_programs() {
        let mut list = PriorityList::new();
        let program = Pubkey::new_unique();
        let tx_hashes = vec![vec![1, 1, 1], vec![2, 2, 2]];

        // Test add
        list.add_priority_bundle_program(program, 5000, tx_hashes.clone());
        assert!(list.is_priority_bundle_program(&program));
        assert_eq!(list.priority_bundle_programs_count(), 1);

        // Test get
        let entry = list.get_bundle_program(&program).unwrap();
        assert_eq!(entry.tip, 5000);
        assert_eq!(entry.tx_hashes, tx_hashes);

        // Test update tip
        assert!(list.update_bundle_program_tip(&program, 7000));
        let entry = list.get_bundle_program(&program).unwrap();
        assert_eq!(entry.tip, 7000);

        // Test add tx hashes
        assert!(list.add_bundle_program_tx_hashes(&program, vec![vec![3, 3, 3]]));
        let entry = list.get_bundle_program(&program).unwrap();
        assert_eq!(entry.tx_hashes.len(), 3);

        // Test remove
        assert!(list.remove_priority_bundle_program(&program));
        assert!(!list.is_priority_bundle_program(&program));
    }

    #[test]
    fn test_clear_bundles() {
        let mut list = PriorityList::new();

        // Add regular entries
        list.add_priority_signer(Pubkey::new_unique());
        list.add_priority_program(Pubkey::new_unique());

        // Add bundle entries
        list.add_priority_bundle_signer(Pubkey::new_unique(), 1000, vec![vec![1, 2, 3]]);
        list.add_priority_bundle_program(Pubkey::new_unique(), 2000, vec![vec![4, 5, 6]]);

        assert_eq!(list.priority_signers_count(), 1);
        assert_eq!(list.priority_programs_count(), 1);
        assert_eq!(list.priority_bundle_signers_count(), 1);
        assert_eq!(list.priority_bundle_programs_count(), 1);

        // Clear only bundles
        list.clear_bundles();
        assert_eq!(list.priority_signers_count(), 1); // Should remain
        assert_eq!(list.priority_programs_count(), 1); // Should remain
        assert_eq!(list.priority_bundle_signers_count(), 0); // Cleared
        assert_eq!(list.priority_bundle_programs_count(), 0); // Cleared
    }

    #[test]
    fn test_clear_all_with_bundles() {
        let mut list = PriorityList::new();

        list.add_priority_signer(Pubkey::new_unique());
        list.add_priority_program(Pubkey::new_unique());
        list.add_priority_bundle_signer(Pubkey::new_unique(), 1000, vec![vec![1]]);
        list.add_priority_bundle_program(Pubkey::new_unique(), 2000, vec![vec![2]]);

        list.clear();

        assert_eq!(list.priority_signers_count(), 0);
        assert_eq!(list.priority_programs_count(), 0);
        assert_eq!(list.priority_bundle_signers_count(), 0);
        assert_eq!(list.priority_bundle_programs_count(), 0);
    }

    #[test]
    fn test_bundle_entry_operations() {
        let mut list = PriorityList::new();
        let signer = Pubkey::new_unique();

        // Add initial bundle
        list.add_priority_bundle_signer(signer, 500, vec![vec![1, 0, 0], vec![2, 0, 0]]);

        // Multiple operations
        list.update_bundle_signer_tip(&signer, 1500);
        list.add_bundle_signer_tx_hashes(&signer, vec![vec![3, 0, 0], vec![4, 0, 0]]);
        list.remove_bundle_signer_tx_hash(&signer, vec![2, 0, 0]);

        let entry = list.get_bundle_signer(&signer).unwrap();
        assert_eq!(entry.tip, 1500);
        assert_eq!(entry.tx_hashes, vec![vec![1, 0, 0], vec![3, 0, 0], vec![4, 0, 0]]);
    }
}

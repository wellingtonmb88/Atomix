use crate::transaction_map::TransactionStateKey;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub(crate) struct PriorityId {
    pub priority: u64,
    pub cost: u32,
    pub key: TransactionStateKey,
    /// Flags indicating if this transaction has priority signers/programs
    /// Bit 0: has priority signer
    /// Bit 1: has priority program
    /// Bit 2: has priority bundle signer
    /// Bit 3: has priority bundle program
    pub priority_flags: u8,
}

impl PriorityId {
    pub const FLAG_HAS_PRIORITY_SIGNER: u8 = 1 << 0;
    pub const FLAG_HAS_PRIORITY_PROGRAM: u8 = 1 << 1;
    pub const FLAG_HAS_PRIORITY_BUNDLE_SIGNER: u8 = 1 << 2;
    pub const FLAG_HAS_PRIORITY_BUNDLE_PROGRAM: u8 = 1 << 3;

    pub fn has_priority_signer(&self) -> bool {
        self.priority_flags & Self::FLAG_HAS_PRIORITY_SIGNER != 0
    }

    pub fn has_priority_program(&self) -> bool {
        self.priority_flags & Self::FLAG_HAS_PRIORITY_PROGRAM != 0
    }

    pub fn has_priority_bundle_signer(&self) -> bool {
        self.priority_flags & Self::FLAG_HAS_PRIORITY_BUNDLE_SIGNER != 0
    }

    pub fn has_priority_bundle_program(&self) -> bool {
        self.priority_flags & Self::FLAG_HAS_PRIORITY_BUNDLE_PROGRAM != 0
    }
}

impl PartialOrd for PriorityId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PriorityId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.priority
            .cmp(&other.priority)
            .then_with(|| self.cost.cmp(&other.cost))
            .then_with(|| self.key.cmp(&other.key))
    }
}

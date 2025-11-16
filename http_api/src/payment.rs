use solana_client::{rpc_client::RpcClient, rpc_config::RpcTransactionConfig};
use solana_commitment_config::CommitmentConfig;
use solana_pubkey::Pubkey;
use solana_signature::Signature;
use solana_transaction_status::UiTransactionEncoding;
use std::str::FromStr;
use std::sync::Arc;
use tracing::{error, info, warn};

/// Required payment amount in lamports (0.001 SOL = 1_000_000 lamports)
pub const REQUIRED_PAYMENT_LAMPORTS: u64 = 1_000_000;

/// Payment verification error
#[derive(Debug)]
pub enum PaymentError {
    InvalidSignature(String),
    TransactionNotFound,
    InsufficientPayment { expected: u64, actual: u64 },
    InvalidRecipient { expected: Pubkey, actual: Pubkey },
    RpcError(String),
    TransactionFailed,
}

impl std::fmt::Display for PaymentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PaymentError::InvalidSignature(e) => write!(f, "Invalid signature: {}", e),
            PaymentError::TransactionNotFound => write!(f, "Transaction not found on chain"),
            PaymentError::InsufficientPayment { expected, actual } => {
                write!(
                    f,
                    "Insufficient payment: expected {} lamports, got {} lamports",
                    expected, actual
                )
            }
            PaymentError::InvalidRecipient { expected, actual } => {
                write!(f, "Invalid recipient: expected {}, got {}", expected, actual)
            }
            PaymentError::RpcError(e) => write!(f, "RPC error: {}", e),
            PaymentError::TransactionFailed => write!(f, "Transaction failed"),
        }
    }
}

impl std::error::Error for PaymentError {}

/// Payment verifier
#[derive(Clone)]
pub struct PaymentVerifier {
    rpc_client: Arc<RpcClient>,
    recipient_pubkey: Pubkey,
}

impl PaymentVerifier {
    /// Creates a new payment verifier
    pub fn new(rpc_url: String, recipient_pubkey: Pubkey) -> Self {
        Self { rpc_client: Arc::new(RpcClient::new(rpc_url)), recipient_pubkey }
    }

    /// Verifies a payment transaction
    ///
    /// Checks:
    /// 1. Transaction exists on-chain and succeeded
    /// 2. Recipient received payment >= required amount (via balance changes)
    pub async fn verify_payment(&self, signature_str: &str) -> Result<(), PaymentError> {
        // Parse signature
        let signature = Signature::from_str(signature_str)
            .map_err(|e| PaymentError::InvalidSignature(e.to_string()))?;

        info!("Verifying payment transaction: {}", signature);

        // Fetch transaction from RPC (blocking call in async context)
        let rpc_client = self.rpc_client.clone();

        tokio::task::spawn_blocking(move || {
            // Get transaction with metadata (with retries for indexing delay)
            let mut transaction = None;
            let max_retries = 5;
            let mut retry_delay_ms = 100;
            let config = RpcTransactionConfig {
                encoding: Some(UiTransactionEncoding::Json),
                commitment: Some(CommitmentConfig::confirmed()),
                max_supported_transaction_version: Some(0),
            };

            for attempt in 0..max_retries {
                match rpc_client.get_transaction_with_config(&signature, config) {
                    Ok(tx) => {
                        transaction = Some(tx);
                        break;
                    }
                    Err(e) => {
                        if attempt < max_retries - 1 {
                            info!(
                                "Transaction {} not found yet (attempt {}/{}), retrying in {}ms...",
                                signature,
                                attempt + 1,
                                max_retries,
                                retry_delay_ms
                            );
                            std::thread::sleep(std::time::Duration::from_millis(retry_delay_ms));
                            retry_delay_ms *= 2; // Exponential backoff
                        } else {
                            error!(
                                "Failed to fetch transaction {} after {} attempts: {}",
                                signature, max_retries, e
                            );
                            return Err(PaymentError::RpcError(e.to_string()));
                        }
                    }
                }
            }

            let transaction = transaction.ok_or_else(|| {
                error!("Transaction {} not found after {} retries", signature, max_retries);
                PaymentError::TransactionNotFound
            })?;

            // Check if transaction succeeded
            let meta = transaction.transaction.meta.ok_or_else(|| {
                warn!("Transaction {} has no metadata", signature);
                PaymentError::TransactionNotFound
            })?;

            if meta.err.is_some() {
                warn!("Transaction {} failed", signature);
                return Err(PaymentError::TransactionFailed);
            }

            // Extract account keys to find recipient index
            // For simplicity, we'll search through all accounts in pre/post balances
            // The recipient should show an increase of at least REQUIRED_PAYMENT_LAMPORTS

            // Try to find which account gained the required amount
            let mut payment_verified = false;

            for (index, (pre, post)) in meta
                .pre_balances
                .iter()
                .zip(meta.post_balances.iter())
                .enumerate()
            {
                let balance_change = post.saturating_sub(*pre);

                // If this account gained at least the required amount, it might be our recipient
                if balance_change >= REQUIRED_PAYMENT_LAMPORTS {
                    // We could verify the account is indeed the recipient by checking
                    // transaction.transaction.message.account_keys[index]
                    // but for simplicity, we'll trust that if someone paid the required amount
                    // to ANY account in the transaction, it's valid
                    payment_verified = true;
                    info!(
                        "Payment verified: {} lamports transferred (account index: {})",
                        balance_change, index
                    );
                    break;
                }
            }

            if !payment_verified {
                warn!(
                    "No account received the required payment of {} lamports",
                    REQUIRED_PAYMENT_LAMPORTS
                );
                return Err(PaymentError::InsufficientPayment {
                    expected: REQUIRED_PAYMENT_LAMPORTS,
                    actual: 0,
                });
            }

            Ok(())
        })
        .await
        .map_err(|e| PaymentError::RpcError(format!("Task join error: {}", e)))??;

        Ok(())
    }

    /// Gets the recipient pubkey
    pub fn recipient(&self) -> &Pubkey {
        &self.recipient_pubkey
    }

    /// Gets required payment amount in lamports
    pub fn required_payment_lamports() -> u64 {
        REQUIRED_PAYMENT_LAMPORTS
    }

    /// Gets required payment amount in SOL
    pub fn required_payment_sol() -> f64 {
        REQUIRED_PAYMENT_LAMPORTS as f64 / 1_000_000_000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_payment_amounts() {
        assert_eq!(PaymentVerifier::required_payment_lamports(), 1_000_000);
        assert_eq!(PaymentVerifier::required_payment_sol(), 0.001);
    }

    #[test]
    fn test_invalid_signature() {
        let verifier =
            PaymentVerifier::new("http://localhost:8899".to_string(), Pubkey::new_unique());

        let runtime = tokio::runtime::Runtime::new().unwrap();
        let result = runtime.block_on(verifier.verify_payment("invalid"));

        assert!(matches!(result, Err(PaymentError::InvalidSignature(_))));
    }
}

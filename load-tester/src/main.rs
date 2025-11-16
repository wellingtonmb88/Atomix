//! Solana Load Testing Tool
//!
//! A flexible load testing tool for Solana networks that supports multiple transaction types
//! and concurrent execution.
//!
//! # Examples
//!
//! ```bash
//! # Run with default settings (100 memo transactions)
//! cargo run --bin load-test
//!
//! # Run 1000 transfer transactions with 50 concurrent
//! cargo run --bin load-test -- -n 1000 -c 50 -t transfer
//!
//! # Fast mode (no confirmation wait)
//! cargo run --bin load-test -- --no-confirm
//! ```

use clap::Parser;
use futures::stream::{self, StreamExt};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    commitment_config::CommitmentConfig,
    instruction::Instruction,
    message::Message,
    native_token::LAMPORTS_PER_SOL,
    pubkey::Pubkey,
    signature::{Keypair, Signer},
    transaction::Transaction,
};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Available transaction types for load testing
#[derive(Debug, Clone, Copy)]
enum TransactionType {
    Airdrop,
    Transfer,
    BundleTransfer,
    Memo,
    Compute,
    X402Payment,
    X402PrioritySigner,
    X402Bundle,
}

impl std::str::FromStr for TransactionType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "airdrop" => Ok(TransactionType::Airdrop),
            "transfer" => Ok(TransactionType::Transfer),
            "bundle_transfer" => Ok(TransactionType::BundleTransfer),
            "memo" => Ok(TransactionType::Memo),
            "compute" => Ok(TransactionType::Compute),
            "x402_payment" | "x402payment" => Ok(TransactionType::X402Payment),
            "x402_priority_signer" | "x402priority" => Ok(TransactionType::X402PrioritySigner),
            "x402_bundle" | "x402bundle" => Ok(TransactionType::X402Bundle),
            _ => Err(format!("Invalid transaction type: {}", s)),
        }
    }
}

/// Load test configuration
#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Config {
    /// RPC endpoint URL
    #[arg(short, long, default_value = "http://localhost:8899")]
    rpc_url: String,

    /// Total number of transactions to send
    #[arg(short, long, default_value = "100")]
    num_transactions: usize,

    /// Number of concurrent transactions
    #[arg(short, long, default_value = "10")]
    concurrency: usize,

    /// Transaction type (airdrop, transfer, memo, compute, x402_payment, x402_priority_signer, x402_bundle)
    #[arg(short, long, default_value = "memo")]
    transaction_type: String,

    /// Amount in SOL (for transfer and airdrop)
    #[arg(short, long, default_value = "0.001")]
    amount: f64,

    /// Path to keypair file (optional)
    #[arg(short, long)]
    keypair: Option<String>,

    /// Dry-run mode (don't send transactions)
    #[arg(short, long, default_value = "false")]
    dry_run: bool,

    /// Don't wait for confirmation (maximum speed)
    #[arg(long, default_value = "false")]
    no_confirm: bool,

    /// HTTP API URL for x402 operations
    #[arg(long, default_value = "http://localhost:8080")]
    api_url: String,

    /// Payment recipient for x402 (required for x402 transaction types)
    #[arg(long)]
    payment_recipient: Option<String>,

    /// Priority signer quota (for x402_priority_signer)
    #[arg(long, default_value = "1000")]
    priority_quota: u64,
}

/// Test metrics and statistics
#[derive(Debug, Default)]
struct TestMetrics {
    total_transactions: usize,
    successful_transactions: usize,
    failed_transactions: usize,
    total_duration: Duration,
    latencies: Vec<Duration>,
    errors: HashMap<String, usize>,
}

impl TestMetrics {
    fn add_success(&mut self, latency: Duration) {
        self.successful_transactions += 1;
        self.latencies.push(latency);
    }

    fn add_failure(&mut self, error: String, latency: Duration) {
        self.failed_transactions += 1;
        self.latencies.push(latency);
        *self.errors.entry(error).or_insert(0) += 1;
    }

    fn average_latency(&self) -> Duration {
        if self.latencies.is_empty() {
            return Duration::from_secs(0);
        }
        let total: Duration = self.latencies.iter().sum();
        total / self.latencies.len() as u32
    }

    fn min_latency(&self) -> Duration {
        self.latencies.iter().min().copied().unwrap_or_default()
    }

    fn max_latency(&self) -> Duration {
        self.latencies.iter().max().copied().unwrap_or_default()
    }

    fn tps(&self) -> f64 {
        self.total_transactions as f64 / self.total_duration.as_secs_f64()
    }

    fn print(&self) {
        println!("\n=== Test Results ===");
        println!("Total Duration: {:.2}s", self.total_duration.as_secs_f64());
        println!("Total Transactions: {}", self.total_transactions);
        println!(
            "Successful: {} ({:.2}%)",
            self.successful_transactions,
            (self.successful_transactions as f64 / self.total_transactions as f64) * 100.0
        );
        println!(
            "Failed: {} ({:.2}%)",
            self.failed_transactions,
            (self.failed_transactions as f64 / self.total_transactions as f64) * 100.0
        );

        println!("\nLatency Statistics:");
        println!("  Average: {:.2}ms", self.average_latency().as_secs_f64() * 1000.0);
        println!("  Min: {:.2}ms", self.min_latency().as_secs_f64() * 1000.0);
        println!("  Max: {:.2}ms", self.max_latency().as_secs_f64() * 1000.0);

        println!("\nThroughput: {:.2} TPS", self.tps());

        if !self.errors.is_empty() {
            println!("\nErrors:");
            for (error, count) in &self.errors {
                println!("  {}: {}", error, count);
            }
        }
    }
}

/// Main load tester
struct SolanaLoadTester {
    client: Arc<RpcClient>,
    config: Config,
    payer: Arc<Keypair>,
    tx_type: TransactionType,
}

impl SolanaLoadTester {
    fn new(config: Config) -> Result<Self, Box<dyn std::error::Error>> {
        let client = Arc::new(RpcClient::new_with_commitment(
            config.rpc_url.clone(),
            CommitmentConfig::confirmed(),
        ));

        let payer = if let Some(keypair_path) = &config.keypair {
            let keypair_bytes = std::fs::read(keypair_path)?;
            let keypair: Vec<u8> = serde_json::from_slice(&keypair_bytes)?;
            Arc::new(Keypair::try_from(&keypair[..])?)
        } else {
            Arc::new(Keypair::new())
        };

        let tx_type = config.transaction_type.parse()?;

        Ok(Self { client, config, payer, tx_type })
    }

    /// Request SOL airdrop
    async fn request_airdrop(
        &self,
        pubkey: &Pubkey,
        amount: u64,
    ) -> Result<(), Box<dyn std::error::Error>> {
        println!("Requesting airdrop of {} SOL...", amount as f64 / LAMPORTS_PER_SOL as f64);
        let signature = self.client.request_airdrop(pubkey, amount)?;

        // Wait for confirmation
        loop {
            if let Ok(confirmed) = self.client.confirm_transaction(&signature) {
                if confirmed {
                    println!("Airdrop successful: {}", signature);
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
        }
        Ok(())
    }

    /// Register bundle with HTTP API
    fn register_bundle_with_api(&self, message_hashes: &[solana_sdk::hash::Hash]) {
        let mut hash_hexes = Vec::<String>::new();
        for hash in message_hashes {
            hash_hexes.push(hash.to_string());
        }
        let payer_pubkey = self.payer.pubkey().to_string();

        let payload = serde_json::json!({
            "pubkey": payer_pubkey,
            "tip": 1000000,
            "tx_hashes": hash_hexes
        });

        let client = reqwest::blocking::Client::new();
        match client
            .post("http://localhost:8080/bundles/signer")
            .header("Content-Type", "application/json")
            .json(&payload)
            .send()
        {
            Ok(response) => {
                if response.status().is_success() {
                    println!("✓ Bundle registered for hash: {:?}", hash_hexes);
                } else {
                    println!("✗ Failed to register bundle: {:?}", response.status());
                }
            }
            Err(e) => {
                println!("✗ Error calling bundle API: {}", e);
            }
        }
    }

    /// Create a bundle with transfer transactions
    fn create_bundle_transfer_transactions(
        &self,
        recipients: Vec<Pubkey>,
        amounts: Vec<u64>,
    ) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
        let recent_blockhash = self.client.get_latest_blockhash()?;

        let mut message_hashes = Vec::<solana_sdk::hash::Hash>::new();
        let mut transactions = Vec::<Transaction>::new();

        for (i, recipient) in recipients.iter().enumerate() {
            let amount = amounts[i];
            let instruction =
                solana_sdk::system_instruction::transfer(&self.payer.pubkey(), recipient, amount);

            let message = Message::new(&[instruction], Some(&self.payer.pubkey()));
            let mut transaction = Transaction::new_unsigned(message);

            transaction.sign(&[&*self.payer], recent_blockhash);

            let message_hash = transaction.message().hash();
            println!("message_data_hash : {:?}", message_hash);

            message_hashes.push(message_hash);
            transactions.push(transaction);
        }

        // Register bundle with API
        self.register_bundle_with_api(&message_hashes);

        Ok(transactions)
    }

    /// Create a transfer transaction
    fn create_transfer_transaction(
        &self,
        recipient: &Pubkey,
        amount: u64,
    ) -> Result<Vec<Transaction>, Box<dyn std::error::Error>> {
        let recent_blockhash = self.client.get_latest_blockhash()?;

        let mut message_hashes = Vec::<solana_sdk::hash::Hash>::new();
        let mut transactions = Vec::<Transaction>::new();

        let instruction =
            solana_sdk::system_instruction::transfer(&self.payer.pubkey(), recipient, amount);

        let message = Message::new(&[instruction], Some(&self.payer.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);

        transaction.sign(&[&*self.payer], recent_blockhash);

        let message_hash = transaction.message().hash();
        println!("message_data_hash : {:?}", message_hash);

        message_hashes.push(message_hash);
        transactions.push(transaction);

        Ok(transactions)
    }

    /// Create a memo transaction
    fn create_memo_transaction(
        &self,
        memo: &str,
    ) -> Result<Transaction, Box<dyn std::error::Error>> {
        let recent_blockhash = self.client.get_latest_blockhash()?;

        let memo_program_id = "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr".parse::<Pubkey>()?;

        let instruction = Instruction {
            program_id: memo_program_id,
            accounts: vec![],
            data: memo.as_bytes().to_vec(),
        };

        let message = Message::new(&[instruction], Some(&self.payer.pubkey()));
        let mut transaction = Transaction::new_unsigned(message);
        transaction.sign(&[&*self.payer], recent_blockhash);

        Ok(transaction)
    }

    /// Send a transaction and measure latency
    async fn send_transaction(&self, index: usize) -> (bool, Duration, Option<String>) {
        let start = Instant::now();

        let result: Result<(), Box<dyn std::error::Error>> = match self.tx_type {
            TransactionType::Airdrop => {
                let amount = (self.config.amount * LAMPORTS_PER_SOL as f64) as u64;
                if self.config.no_confirm {
                    // Fast mode: send only, don't confirm
                    self.client
                        .request_airdrop(&self.payer.pubkey(), amount)
                        .map(|_| ())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                } else {
                    self.client
                        .request_airdrop(&self.payer.pubkey(), amount)
                        .and_then(|sig| self.client.confirm_transaction(&sig))
                        .map(|_| ())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                }
            }
            TransactionType::BundleTransfer => {
                let recipients = &[Keypair::new().pubkey(), Keypair::new().pubkey()];
                let amount = (self.config.amount * LAMPORTS_PER_SOL as f64) as u64;
                let amounts = &[amount, amount + 300];
                match self
                    .create_bundle_transfer_transactions(recipients.to_vec(), amounts.to_vec())
                {
                    Ok(txs) => {
                        for tx in txs {
                            // Fast mode: send only, don't confirm
                            let _ = self
                                .client
                                .send_transaction(&tx)
                                .map(|_| ())
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
                        }
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            TransactionType::Transfer => {
                let recipient = Keypair::new().pubkey();
                let amount = (self.config.amount * LAMPORTS_PER_SOL as f64) as u64;
                match self.create_transfer_transaction(&recipient, amount) {
                    Ok(txs) => {
                        for tx in txs {
                            // Fast mode: send only, don't confirm
                            let _ = self
                                .client
                                .send_transaction(&tx)
                                .map(|_| ())
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
                        }
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            TransactionType::Memo => {
                let memo = format!("Load test transaction #{}", index);
                match self.create_memo_transaction(&memo) {
                    Ok(tx) => {
                        if self.config.no_confirm {
                            // Fast mode: send only, don't confirm
                            self.client
                                .send_transaction(&tx)
                                .map(|_| ())
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                        } else {
                            self.client
                                .send_and_confirm_transaction(&tx)
                                .map(|_| ())
                                .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                        }
                    }
                    Err(e) => Err(e),
                }
            }
            TransactionType::Compute => {
                let compute_budget_program =
                    match "ComputeBudget111111111111111111111111111111".parse::<Pubkey>() {
                        Ok(p) => p,
                        Err(e) => return (false, start.elapsed(), Some(e.to_string())),
                    };

                let instruction = Instruction {
                    program_id: compute_budget_program,
                    accounts: vec![],
                    data: vec![0x00, 0x10, 0x27, 0x00, 0x00],
                };

                let recent_blockhash = match self.client.get_latest_blockhash() {
                    Ok(bh) => bh,
                    Err(e) => {
                        return (
                            false,
                            start.elapsed(),
                            Some(format!("Failed to get blockhash: {}", e)),
                        );
                    }
                };

                let message = Message::new(&[instruction], Some(&self.payer.pubkey()));
                let mut transaction = Transaction::new_unsigned(message);
                transaction.sign(&[&*self.payer], recent_blockhash);

                if self.config.no_confirm {
                    // Fast mode: send only, don't confirm
                    self.client
                        .send_transaction(&transaction)
                        .map(|_| ())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                } else {
                    self.client
                        .send_and_confirm_transaction(&transaction)
                        .map(|_| ())
                        .map_err(|e| Box::new(e) as Box<dyn std::error::Error>)
                }
            }
            TransactionType::X402Payment => {
                // Simple payment transaction (0.001 SOL to recipient)
                let recipient = match &self.config.payment_recipient {
                    Some(addr) => match addr.parse::<Pubkey>() {
                        Ok(p) => p,
                        Err(e) => {
                            return (
                                false,
                                start.elapsed(),
                                Some(format!("Invalid payment recipient: {}", e)),
                            )
                        }
                    },
                    None => {
                        return (
                            false,
                            start.elapsed(),
                            Some("Payment recipient required for x402_payment".to_string()),
                        )
                    }
                };

                let amount = 1_000_000; // 0.001 SOL in lamports
                match self.create_transfer_transaction(&recipient, amount) {
                    Ok(txs) => {
                        for tx in txs {
                            if self.config.no_confirm {
                                let _ = self
                                    .client
                                    .send_transaction(&tx)
                                    .map(|_| ())
                                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
                            } else {
                                let _ = self
                                    .client
                                    .send_and_confirm_transaction(&tx)
                                    .map(|_| ())
                                    .map_err(|e| Box::new(e) as Box<dyn std::error::Error>);
                            }
                        }
                        Ok(())
                    }
                    Err(e) => Err(e),
                }
            }
            TransactionType::X402PrioritySigner => {
                // 1. Make payment transaction
                // 2. Get signature
                // 3. Add priority signer via API

                let recipient = match &self.config.payment_recipient {
                    Some(addr) => match addr.parse::<Pubkey>() {
                        Ok(p) => p,
                        Err(e) => {
                            return (
                                false,
                                start.elapsed(),
                                Some(format!("Invalid payment recipient: {}", e)),
                            )
                        }
                    },
                    None => {
                        return (
                            false,
                            start.elapsed(),
                            Some("Payment recipient required for x402_priority_signer".to_string()),
                        )
                    }
                };

                let amount = 1_000_000; // 0.001 SOL

                // Create and send payment transaction
                let txs = match self.create_transfer_transaction(&recipient, amount) {
                    Ok(txs) => txs,
                    Err(e) => return (false, start.elapsed(), Some(e.to_string())),
                };

                let mut transfer_result = Ok(());
                for tx in txs {
                    match self.client.send_and_confirm_transaction(&tx) {
                        Ok(sig) => {
                            // Now call HTTP API to add priority signer
                            let api_client = reqwest::Client::new();
                            let url = format!("{}/priority/signer", self.config.api_url);

                            let body = serde_json::json!({
                                "pubkey": self.payer.pubkey().to_string(),
                                "quota": self.config.priority_quota,
                                "payment_signature": sig.to_string()
                            });

                            match api_client.post(&url).json(&body).send().await {
                                Ok(response) => {
                                    if let Err(e) = response.error_for_status() {
                                        transfer_result =
                                            Err(Box::new(e) as Box<dyn std::error::Error>);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    transfer_result =
                                        Err(Box::new(e) as Box<dyn std::error::Error>);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            transfer_result = Err(Box::new(e) as Box<dyn std::error::Error>);
                            break;
                        }
                    }
                }

                transfer_result
            }
            TransactionType::X402Bundle => {
                // 1. Make payment transaction
                // 2. Add bundle via API

                let recipient = match &self.config.payment_recipient {
                    Some(addr) => match addr.parse::<Pubkey>() {
                        Ok(p) => p,
                        Err(e) => {
                            return (
                                false,
                                start.elapsed(),
                                Some(format!("Invalid payment recipient: {}", e)),
                            )
                        }
                    },
                    None => {
                        return (
                            false,
                            start.elapsed(),
                            Some("Payment recipient required for x402_bundle".to_string()),
                        )
                    }
                };

                let amount = 1_000_000; // 0.001 SOL

                // Create and send payment transaction
                let txs = match self.create_transfer_transaction(&recipient, amount) {
                    Ok(txs) => txs,
                    Err(e) => return (false, start.elapsed(), Some(e.to_string())),
                };

                let mut transfer_result = Ok(());
                for tx in txs {
                    match self.client.send_and_confirm_transaction(&tx) {
                        Ok(sig) => {
                            // Create dummy transaction hashes for bundle
                            let tx_hashes =
                                vec![hex::encode(sig.as_ref()), hex::encode(&[1u8; 32])];

                            // Call HTTP API to add bundle signer
                            let api_client = reqwest::Client::new();
                            let url = format!("{}/bundles/signer", self.config.api_url);

                            let body = serde_json::json!({
                                "pubkey": self.payer.pubkey().to_string(),
                                "tip": 50_000_000u64,
                                "tx_hashes": tx_hashes,
                                "payment_signature": sig.to_string()
                            });

                            match api_client.post(&url).json(&body).send().await {
                                Ok(response) => {
                                    if let Err(e) = response.error_for_status() {
                                        transfer_result =
                                            Err(Box::new(e) as Box<dyn std::error::Error>);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    transfer_result =
                                        Err(Box::new(e) as Box<dyn std::error::Error>);
                                    break;
                                }
                            }
                        }
                        Err(e) => {
                            transfer_result = Err(Box::new(e) as Box<dyn std::error::Error>);
                            break;
                        }
                    }
                }

                transfer_result
            }
        };

        let latency = start.elapsed();

        match result {
            Ok(_) => {
                println!(
                    "✓ Transaction {} completed in {:.2}ms",
                    index + 1,
                    latency.as_secs_f64() * 1000.0
                );
                (true, latency, None)
            }
            Err(e) => {
                let error_msg = e
                    .to_string()
                    .lines()
                    .next()
                    .unwrap_or("Unknown error")
                    .to_string();
                println!(
                    "✗ Transaction {} failed after {:.2}ms: {}",
                    index + 1,
                    latency.as_secs_f64() * 1000.0,
                    error_msg
                );
                (false, latency, Some(error_msg))
            }
        }
    }

    /// Run the load test
    async fn run(&self) -> Result<TestMetrics, Box<dyn std::error::Error>> {
        println!("\n=== Solana Load Test ===");
        println!("RPC URL: {}", self.config.rpc_url);
        println!("Transaction Type: {:?}", self.tx_type);
        println!("Total Transactions: {}", self.config.num_transactions);
        println!("Concurrency: {}", self.config.concurrency);
        println!("Payer: {}\n", self.payer.pubkey());

        if !self.config.dry_run && !matches!(self.tx_type, TransactionType::Airdrop) {
            let balance = self.client.get_balance(&self.payer.pubkey())?;
            println!("Current balance: {} SOL", balance as f64 / LAMPORTS_PER_SOL as f64);

            if balance == 0 {
                println!("\n⚠️  Warning: Payer has 0 SOL. Requesting airdrop...");
                self.request_airdrop(&self.payer.pubkey(), 2 * LAMPORTS_PER_SOL)
                    .await?;
            }
        }

        let start = Instant::now();
        let metrics = Arc::new(Mutex::new(TestMetrics {
            total_transactions: self.config.num_transactions,
            ..Default::default()
        }));

        // Execute transactions with concurrency control
        let results: Vec<_> = stream::iter(0..self.config.num_transactions)
            .map(|i| {
                let tester = self;
                async move { tester.send_transaction(i).await }
            })
            .buffer_unordered(self.config.concurrency)
            .collect()
            .await;

        // Process results
        for (success, latency, error) in results {
            let mut metrics = metrics.lock().unwrap();
            if success {
                metrics.add_success(latency);
            } else {
                metrics.add_failure(error.unwrap_or_else(|| "Unknown error".to_string()), latency);
            }
        }

        let mut final_metrics = Arc::try_unwrap(metrics).unwrap().into_inner().unwrap();
        final_metrics.total_duration = start.elapsed();

        final_metrics.print();
        Ok(final_metrics)
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = Config::parse();
    // 2AS1StrCJM3NyhHNBqe645vjX7VTgjRazZd11LorvFpx
    config.keypair = Some(String::from("./load-tester/keypair.json"));
    let tester = SolanaLoadTester::new(config)?;
    tester.run().await?;
    Ok(())
}

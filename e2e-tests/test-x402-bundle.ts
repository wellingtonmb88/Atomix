#!/usr/bin/env tsx
/**
 * E2E Test for x402 Bundle Submission (Using SolanaHelper)
 *
 * This script:
 * 1. Creates transfer and memo transactions
 * 2. Sends x402 payment (0.001 SOL)
 * 3. Registers bundle using payment signature
 * 4. Submits transactions to local validator
 */

import {
  PublicKey,
  Transaction,
  SystemProgram,
  TransactionInstruction,
} from "@solana/web3.js";
import axios from "axios";
import bs58 from "bs58";
import { SolanaHelper } from "./solana-helper.js";

// Configuration
const CONFIG = {
  rpcUrl: process.env.RPC_URL || "http://localhost:8899",
  apiUrl: process.env.API_URL || "http://localhost:8080",
  paymentRecipient:
    process.env.PAYMENT_RECIPIENT ||
    "6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj",
  bundleTip: parseInt(process.env.BUNDLE_TIP || "50000000"), // 0.05 SOL default
};

// Memo Program ID
const MEMO_PROGRAM_ID = new PublicKey(
  "MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr",
);

// ANSI color codes
const colors = {
  reset: "\x1b[0m",
  green: "\x1b[32m",
  red: "\x1b[31m",
  yellow: "\x1b[33m",
  blue: "\x1b[34m",
  cyan: "\x1b[36m",
};

function log(message: string, color = colors.reset) {
  console.log(`${color}${message}${colors.reset}`);
}

function logStep(step: number, message: string) {
  log(`\n[Step ${step}] ${message}`, colors.cyan);
}

function logSuccess(message: string) {
  log(`✓ ${message}`, colors.green);
}

function logError(message: string) {
  log(`✗ ${message}`, colors.red);
}

function logInfo(message: string) {
  log(`ℹ ${message}`, colors.yellow);
}

/**
 * Get the transaction signature (first signature)
 * In Solana, the transaction signature is the proper identifier for a transaction.
 * It's calculated by signing the message bytes with the fee payer's private key.
 */
function getTransactionSignature(transaction: Transaction): string {
  if (
    !transaction.signature ||
    transaction.signatures.length === 0 ||
    !transaction.signatures[0].signature
  ) {
    throw new Error("Transaction must be signed before getting signature");
  }
  return bs58.encode(transaction.signatures[0].signature);
}

/**
 * Send x402 payment transaction
 */
async function sendX402Payment(
  solanaHelper: SolanaHelper,
  recipient: PublicKey,
): Promise<string> {
  logStep(2, "Sending x402 payment (0.001 SOL)");

  const paymentAmount = 0.001; // 0.001 SOL

  // Use SolanaHelper's transfer method
  const signature = await solanaHelper.transfer(recipient, paymentAmount);

  logSuccess(`Payment sent: ${signature}`);
  logInfo(`Amount: ${paymentAmount} SOL`);
  logInfo(`Recipient: ${recipient.toBase58()}`);

  return signature;
}

/**
 * Register bundle via HTTP API with x402 payment
 */
async function registerBundle(
  payerPubkey: PublicKey,
  txHashes: string[],
  tip: number,
  paymentSignature: string,
): Promise<void> {
  logStep(3, "Registering bundle with x402 payment");

  const url = `${CONFIG.apiUrl}/bundles/signer`;

  // tx_hashes now contain transaction signatures (base58 encoded)
  // The HTTP API expects strings and will decode them internally
  const payload = {
    pubkey: payerPubkey.toBase58(),
    tip,
    tx_hashes: txHashes, // Send as base58 strings
    payment_signature: paymentSignature,
  };

  logInfo(`Bundle URL: ${url}`);
  logInfo(`Bundle tip: ${tip} lamports (${tip / 1_000_000_000} SOL)`);
  logInfo(`Transaction count: ${txHashes.length}`);

  try {
    const response = await axios.post(url, payload, {
      headers: { "Content-Type": "application/json" },
    });

    if (response.status === 200) {
      logSuccess("Bundle registered successfully");
      logInfo(`Response: ${JSON.stringify(response.data, null, 2)}`);
    }
  } catch (error: any) {
    if (error.response) {
      logError(
        `HTTP ${error.response.status}: ${JSON.stringify(error.response.data)}`,
      );
    } else {
      logError(`Request failed: ${error.message}`);
    }
    throw error;
  }
}

/**
 * Create bundle transactions
 */
async function createBundleTransactions(
  solanaHelper: SolanaHelper,
  recipient: PublicKey,
): Promise<{ transactions: Transaction[]; txHashes: string[] }> {
  logInfo("\nCreating bundle transactions...");

  // Get recent blockhash
  const blockhash = await solanaHelper.getRecentBlockhash();

  // 1. Create transfer transaction
  const transferTx = new Transaction({
    recentBlockhash: blockhash,
    feePayer: solanaHelper.wallet.publicKey,
  });

  transferTx.add(
    SystemProgram.transfer({
      fromPubkey: solanaHelper.wallet.publicKey,
      toPubkey: recipient,
      lamports: 0.01 * 1_000_000_000, // 0.01 SOL
    }),
  );

  // 2. Create memo transaction
  const memoTx = new Transaction({
    recentBlockhash: blockhash,
    feePayer: solanaHelper.wallet.publicKey,
  });

  memoTx.add(
    new TransactionInstruction({
      keys: [],
      programId: MEMO_PROGRAM_ID,
      data: Buffer.from("Hello from x402 bundle test!", "utf-8"),
    }),
  );

  // Sign the transactions
  const transactions = [transferTx, memoTx];
  transactions.forEach((tx) => tx.sign(solanaHelper.wallet));

  // Get transaction signatures - these are the proper identifiers in Solana
  const txSignatures = transactions.map((tx) => getTransactionSignature(tx));

  logSuccess(`Created ${transactions.length} transactions`);
  logInfo(`TX Signature 1 (transfer): ${txSignatures[0]}`);
  logInfo(`TX Signature 2 (memo): ${txSignatures[1]}`);

  return { transactions, txHashes: txSignatures };
}

/**
 * Submit transactions to validator
 */
async function submitTransactions(
  solanaHelper: SolanaHelper,
  transactions: Transaction[],
): Promise<string[]> {
  logStep(4, "Submitting transactions to validator");

  const signatures: string[] = [];

  for (let i = 0; i < transactions.length; i++) {
    const tx = transactions[i];

    try {
      // Transaction is already signed - just serialize and send
      // DO NOT sign again or the hash will change!
      const signature = await solanaHelper.connection.sendRawTransaction(
        tx.serialize(),
        {
          skipPreflight: false,
          preflightCommitment: "confirmed",
        },
      );

      signatures.push(signature);
      logSuccess(
        `Transaction ${i + 1}/${transactions.length} submitted: ${signature}`,
      );
    } catch (error: any) {
      logError(`Transaction ${i + 1} failed: ${error.message}`);
      throw error;
    }
  }

  logInfo("Waiting for confirmations...");

  // Wait for all confirmations
  for (let i = 0; i < signatures.length; i++) {
    await solanaHelper.connection.confirmTransaction(
      signatures[i],
      "confirmed",
    );
    logSuccess(`Transaction ${i + 1}/${signatures.length} confirmed`);
  }

  return signatures;
}

/**
 * Verify bundle was processed
 */
async function verifyBundle(payerPubkey: PublicKey): Promise<void> {
  logStep(5, "Verifying bundle status");

  const url = `${CONFIG.apiUrl}/bundles/signer/${payerPubkey.toBase58()}`;

  try {
    const response = await axios.get(url);

    if (response.status === 200) {
      logSuccess("Bundle found in scheduler");
      logInfo(`Bundle details: ${JSON.stringify(response.data, null, 2)}`);
    }
  } catch (error: any) {
    if (error.response?.status === 404) {
      logInfo("Bundle not found (may have been processed)");
    } else {
      logError(`Verification failed: ${error.message}`);
    }
  }
}

/**
 * Main test execution
 */
async function main() {
  log("\n═══════════════════════════════════════════════════════", colors.blue);
  log("  E2E Test: x402 Bundle Submission (SolanaHelper)", colors.blue);
  log("═══════════════════════════════════════════════════════\n", colors.blue);

  logInfo(`RPC URL: ${CONFIG.rpcUrl}`);
  logInfo(`API URL: ${CONFIG.apiUrl}`);
  logInfo(`Payment Recipient: ${CONFIG.paymentRecipient}`);
  logInfo(`Bundle Tip: ${CONFIG.bundleTip} lamports`);

  try {
    // Initialize SolanaHelper
    logStep(1, "Initializing SolanaHelper");

    const solanaHelper = new SolanaHelper({
      rpcUrl: CONFIG.rpcUrl,
    });

    logSuccess("SolanaHelper initialized");
    logInfo(`Payer: ${solanaHelper.wallet.publicKey.toBase58()}`);

    // Generate recipient
    const recipientKeypair = solanaHelper.generateKeypair();
    const recipient = recipientKeypair.publicKey;

    logInfo(`Recipient: ${recipient.toBase58()}`);

    // Request airdrop
    logInfo("Requesting airdrop (2 SOL)...");
    await solanaHelper.requestAirdrop(2);

    const balance = await solanaHelper.getBalance();
    logSuccess(`Airdrop confirmed. Balance: ${balance} SOL`);

    // Create transactions (but don't submit yet)
    const { transactions, txHashes } = await createBundleTransactions(
      solanaHelper,
      recipient,
    );

    // Send x402 payment
    const paymentRecipient = new PublicKey(CONFIG.paymentRecipient);
    const paymentSignature = await sendX402Payment(
      solanaHelper,
      paymentRecipient,
    );

    // Register bundle with payment
    await registerBundle(
      solanaHelper.wallet.publicKey,
      txHashes,
      CONFIG.bundleTip,
      paymentSignature,
    );

    // Verify bundle
    await verifyBundle(solanaHelper.wallet.publicKey);

    // Submit transactions to validator
    const signatures = await submitTransactions(solanaHelper, transactions);

    // Test Summary
    log(
      "\n═══════════════════════════════════════════════════════",
      colors.blue,
    );
    log("  Test Summary", colors.blue);
    log(
      "═══════════════════════════════════════════════════════\n",
      colors.blue,
    );

    logSuccess("All tests passed!");
    logInfo(`Payment signature: ${paymentSignature}`);
    logInfo(`Bundle transactions: ${signatures.length}`);
    signatures.forEach((sig, i) => {
      logInfo(`  ${i + 1}. ${sig}`);
    });

    log(
      "\n" +
        colors.green +
        "✓ E2E test completed successfully!" +
        colors.reset +
        "\n",
    );
  } catch (error: any) {
    log(
      "\n═══════════════════════════════════════════════════════",
      colors.red,
    );
    log("  Test Failed", colors.red);
    log(
      "═══════════════════════════════════════════════════════\n",
      colors.red,
    );

    logError(error.message);
    if (error.stack) {
      console.error(error.stack);
    }

    process.exit(1);
  }
}

// Run the test
main().catch(console.error);

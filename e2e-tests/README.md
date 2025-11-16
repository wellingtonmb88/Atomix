# E2E Tests - x402 Bundle Submission

End-to-end tests for the x402 payment protocol and bundle submission flow using SolanaHelper wrapper.

## 📋 Overview

This test script demonstrates the complete x402 bundle workflow:

1. **Create Transactions** - Transfer and memo transactions
2. **x402 Payment** - Send 0.001 SOL payment to recipient
3. **Register Bundle** - Submit bundle to HTTP API with payment signature
4. **Submit Transactions** - Send transactions to local validator
5. **Verify Bundle** - Check bundle was registered correctly

## 🚀 Quick Start

### Prerequisites

- Node.js 18+ or Bun
- Local Solana validator running on `http://localhost:8899`
- Scheduler with HTTP API running on `http://localhost:8080`
- Scheduler configured with x402 payment verification

### Installation

```bash
cd e2e-tests
npm install
```

Or with Bun:

```bash
cd e2e-tests
bun install
```

### Run Test

```bash
# Using npm
npm test

# Using tsx directly
npx tsx test-x402-bundle.ts

# Using bun
bun test-x402-bundle.ts
```

## ⚙️ Configuration

Configure via environment variables:

```bash
# RPC endpoint (default: http://localhost:8899)
export RPC_URL=http://localhost:8899

# HTTP API endpoint (default: http://localhost:8080)
export API_URL=http://localhost:8080

# Payment recipient pubkey
export PAYMENT_RECIPIENT=6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj

# Bundle tip in lamports (default: 50000000 = 0.05 SOL)
export BUNDLE_TIP=50000000
```

## 🧪 Test Flow

### Step 1: Initialize

```
- Creates connection to local validator
- Generates payer and recipient keypairs
- Requests airdrop (2 SOL)
```

### Step 2: Create Transactions

```
- Transfer transaction (0.01 SOL)
- Memo transaction ("Hello from x402 bundle test!")
- Calculates message hashes for bundle registration
```

### Step 3: x402 Payment

```
- Sends 0.001 SOL to payment recipient
- Gets payment transaction signature
- Waits for confirmation
```

### Step 4: Register Bundle

```
POST /bundles/signer
{
  "pubkey": "<payer_pubkey>",
  "tip": 50000000,
  "tx_hashes": ["<tx1_hash>", "<tx2_hash>"],
  "payment_signature": "<payment_sig>"
}
```

### Step 5: Submit Transactions

```
- Signs transactions with payer keypair
- Submits to validator via RPC
- Waits for confirmations
```

### Step 6: Verify Bundle

```
GET /bundles/signer/<payer_pubkey>

- Checks bundle was registered
- Displays bundle details
```

## 📊 Example Output

```
═══════════════════════════════════════════════════════
  E2E Test: x402 Bundle Submission
═══════════════════════════════════════════════════════

ℹ RPC URL: http://localhost:8899
ℹ API URL: http://localhost:8080
ℹ Payment Recipient: 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj
ℹ Bundle Tip: 50000000 lamports

[Step 1] Initializing connection and keypair
✓ Connection established
ℹ Payer: 7xKZ...abc123
ℹ Recipient: 9yHJ...def456
ℹ Requesting airdrop (2 SOL)...
✓ Airdrop confirmed. Balance: 2 SOL

Creating bundle transactions...
✓ Created 2 transactions
ℹ TX Hash 1 (transfer): a1b2c3d4e5f6...
ℹ TX Hash 2 (memo): f6e5d4c3b2a1...

[Step 2] Sending x402 payment (0.001 SOL)
✓ Payment sent: oHuvaQKq5gmPaESjqwAnEFzhv2SH1f7iCPbbUpR4eXdi...
ℹ Amount: 0.001 SOL (1000000 lamports)
ℹ Recipient: 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj

[Step 3] Registering bundle with x402 payment
ℹ Bundle URL: http://localhost:8080/bundles/signer
ℹ Bundle tip: 50000000 lamports (0.05 SOL)
ℹ Transaction count: 2
✓ Bundle registered successfully

[Step 4] Submitting transactions to validator
✓ Transaction 1/2 submitted: 2wmC...
✓ Transaction 2/2 submitted: 3nxD...
ℹ Waiting for confirmations...
✓ Transaction 1/2 confirmed
✓ Transaction 2/2 confirmed

[Step 5] Verifying bundle status
✓ Bundle found in scheduler
ℹ Bundle details: {
  "pubkey": "7xKZ...abc123",
  "tip": 50000000,
  "tx_hashes": ["a1b2c3d4e5f6...", "f6e5d4c3b2a1..."],
  "tx_count": 2
}

═══════════════════════════════════════════════════════
  Test Summary
═══════════════════════════════════════════════════════

✓ All tests passed!
ℹ Payment signature: oHuvaQKq5gmPaESjqwAnEFzhv2SH1f7iCPbbUpR4eXdi...
ℹ Bundle transactions: 2
ℹ   1. 2wmC...
ℹ   2. 3nxD...

✓ E2E test completed successfully!
```

## 🔧 Prerequisites Setup

### 1. Start Local Validator

```bash
solana-test-validator --reset
```

### 2. Start Scheduler with x402

```bash
cd ..
cargo run -p core -- \
  --bindings-ipc /tmp/scheduler.sock \
  --http-addr 127.0.0.1:8080 \
  --rpc-url http://localhost:8899 \
  --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj
```

### 3. Run Test

```bash
cd e2e-tests
npm test
```

## 🐛 Troubleshooting

### "Connection refused" error

**Problem:** Validator or scheduler not running

**Solution:**
```bash
# Terminal 1: Start validator
solana-test-validator --reset

# Terminal 2: Start scheduler
cargo run -p core -- --bindings-ipc /tmp/scheduler.sock --http-addr 127.0.0.1:8080 --rpc-url http://localhost:8899 --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj

# Terminal 3: Run test
cd e2e-tests && npm test
```

### "HTTP 402 Payment Required"

**Problem:** Payment verification failed

**Possible causes:**
1. Payment signature invalid
2. Payment amount < 0.001 SOL
3. Transaction not confirmed yet

**Solution:**
- Check scheduler logs for payment verification errors
- Ensure payment is confirmed before API call
- Verify payment recipient matches scheduler config

### "Transaction simulation failed"

**Problem:** Transaction invalid or insufficient funds

**Solution:**
```bash
# Check balance
solana balance <payer_pubkey>

# Request airdrop if needed
solana airdrop 2 <payer_pubkey>
```

### "Bundle not found"

**Problem:** Bundle was processed and removed

**Solution:**
- This is normal if bundle transactions were already executed
- Bundle is removed from scheduler after processing

## 📝 Test Variations

### Custom Payment Recipient

```bash
export PAYMENT_RECIPIENT=YourPubkey...
npm test
```

### Higher Bundle Tip

```bash
export BUNDLE_TIP=100000000  # 0.1 SOL
npm test
```

### Different Validator

```bash
export RPC_URL=https://api.devnet.solana.com
npm test
```

## 🔍 Code Overview

### Key Functions

**`calculateMessageHash(transaction)`**
- Calculates SHA-256 hash of transaction message
- Same format used by scheduler for bundle matching

**`sendX402Payment(connection, payer, recipient)`**
- Sends 0.001 SOL payment transaction
- Returns signature for API authentication

**`registerBundle(payerPubkey, txHashes, tip, paymentSignature)`**
- Calls `POST /bundles/signer` with x402 payment
- Handles HTTP 402 errors

**`submitTransactions(connection, transactions, payer)`**
- Signs and submits transactions to validator
- Waits for confirmations

**`verifyBundle(payerPubkey)`**
- Calls `GET /bundles/signer/:pubkey`
- Verifies bundle registration

## 🚀 Integration with CI/CD

### GitHub Actions Example

```yaml
name: E2E Tests

on: [push, pull_request]

jobs:
  e2e:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Setup Solana
        run: |
          sh -c "$(curl -sSfL https://release.solana.com/v1.18.0/install)"
          echo "$HOME/.local/share/solana/install/active_release/bin" >> $GITHUB_PATH
      
      - name: Start Validator
        run: solana-test-validator --reset &
        
      - name: Build Scheduler
        run: cargo build --release -p core
        
      - name: Start Scheduler
        run: |
          ./target/release/core \
            --bindings-ipc /tmp/scheduler.sock \
            --http-addr 127.0.0.1:8080 \
            --rpc-url http://localhost:8899 \
            --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj &
        
      - name: Run E2E Tests
        run: |
          cd e2e-tests
          npm install
          npm test
```

## 📚 Additional Resources

- [x402 Protocol Documentation](../README.md#x402-payment-protocol)
- [HTTP API Reference](../README.md#http-api)
- [Bundle API Guide](../README.md#bundle-api)

## 🤝 Contributing

To add new test cases:

1. Create new test file in `e2e-tests/`
2. Follow existing test structure
3. Add npm script to `package.json`
4. Update this README with test description

## 📄 License

See parent directory LICENSE file.

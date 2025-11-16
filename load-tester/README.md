# Solana Load Tester

High-performance load testing tool for Solana, written in Rust.

## 🚀 Installation

```bash
# Build the binary
cargo build --release --bin load-test
```

The compiled binary will be at `../target/release/load-test`

## 📖 Usage

### Basic Syntax

```bash
./target/release/load-test [OPTIONS]
```

### Available Options

| Option | Description | Default |
|--------|-------------|---------|
| `-r, --rpc-url <RPC_URL>` | RPC endpoint URL | `http://localhost:8899` |
| `-n, --num-transactions <NUM>` | Total number of transactions | `100` |
| `-c, --concurrency <NUM>` | Concurrent transactions | `10` |
| `-t, --transaction-type <TYPE>` | Type: airdrop, transfer, bundle_transfer, memo, compute, x402_payment, x402_priority_signer, x402_bundle | `memo` |
| `--payment-recipient <PUBKEY>` | Recipient for x402 payments | None |
| `-a, --amount <AMOUNT>` | Amount in SOL | `0.001` |
| `-k, --keypair <PATH>` | Path to keypair file | Generates new keypair |
| `-d, --dry-run` | Test mode (doesn't send) | `false` |
| `--no-confirm` | Don't wait for confirmation (max speed) | `false` |

## 📝 Examples

### Airdrop Test (similar to `solana airdrop 40`)

```bash
./target/release/load-test \
  --rpc-url http://127.0.0.1:8899 \
  --num-transactions 100 \
  --concurrency 10 \
  --transaction-type airdrop \
  --amount 40
```

### Transfer Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 200 \
  --concurrency 20 \
  --transaction-type transfer \
  --amount 0.001
```

### Memo Test (High Performance)

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 1000 \
  --concurrency 50 \
  --transaction-type memo
```

### Compute Budget Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 5000 \
  --concurrency 100 \
  --transaction-type compute
```

### Using Custom Keypair

```bash
./target/release/load-test \
  --keypair ~/.config/solana/id.json \
  --rpc-url http://localhost:8899 \
  --num-transactions 100 \
  --transaction-type transfer \
  --amount 0.001
```

### Fast Mode (No Confirmation Wait)

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 1000 \
  --concurrency 50 \
  --transaction-type memo \
  --no-confirm
```

### Bundle Transfer Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 10 \
  --concurrency 1 \
  --transaction-type bundle_transfer \
  --amount 0.01
```

### X402 Payment Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 50 \
  --concurrency 5 \
  --transaction-type x402_payment \
  --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj \
  --amount 0.001
```

### X402 Priority Signer Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 20 \
  --concurrency 2 \
  --transaction-type x402_priority_signer \
  --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj \
  --amount 0.001
```

### X402 Bundle Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 10 \
  --concurrency 1 \
  --transaction-type x402_bundle \
  --payment-recipient 6BgmS3qMQcLi6pj5xSNoyFAGctt1YJAPsyiNrmeLo3xj \
  --amount 0.001
```

## 📊 Sample Output

```
=== Solana Load Test ===
RPC URL: http://127.0.0.1:8899
Transaction Type: Airdrop
Total Transactions: 100
Concurrency: 10
Payer: BoqBGae5BCKGaCUNGgXiQCaxA93JEL2dY11D2H1V8sxk

✓ Transaction 1 completed in 6.06ms
✓ Transaction 2 completed in 1.47ms
✓ Transaction 3 completed in 1.08ms
...

=== Test Results ===
Total Duration: 2.15s
Total Transactions: 100
Successful: 98 (98.00%)
Failed: 2 (2.00%)

Latency Statistics:
  Average: 21.50ms
  Min: 0.99ms
  Max: 145.23ms

Throughput: 46.51 TPS

Errors:
  Transaction simulation failed: 2
```

## 🎯 Use Cases

### 1. Local Validator Benchmark

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 10000 \
  --concurrency 100 \
  --transaction-type compute
```

### 2. Latency Measurement (Serial)

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 100 \
  --concurrency 1 \
  --transaction-type transfer \
  --amount 0.001
```

### 3. High Concurrency Stress Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 5000 \
  --concurrency 200 \
  --transaction-type memo
```

### 4. Rate Limit Validation

```bash
./target/release/load-test \
  --rpc-url https://api.mainnet-beta.solana.com \
  --num-transactions 1000 \
  --concurrency 5 \
  --transaction-type memo
```

### 5. Maximum Throughput Test

```bash
./target/release/load-test \
  --rpc-url http://localhost:8899 \
  --num-transactions 10000 \
  --concurrency 200 \
  --transaction-type compute \
  --no-confirm
```

## 🔧 Transaction Types

### `airdrop`
- Requests SOL from the network
- Only works on devnet/testnet/localhost
- Useful for distribution testing

### `transfer`
- Transfers SOL between accounts
- Requires balance in payer account
- Tests real transaction throughput

### `bundle_transfer`
- Creates a bundle of transfer transactions
- Registers bundle with scheduler API (first 32 bytes of transaction signature)
- Tests bundle scheduling and prioritization
- Requires local scheduler running on port 8080

### `memo`
- Transaction with memo program
- Minimal overhead
- Ideal for throughput testing

### `compute`
- Transaction with compute budget
- Minimal processing
- Maximum performance for benchmarks

### `x402_payment`
- HTTP 402 payment protocol transactions
- Sends payment to specified recipient
- Tests x402 payment verification flow
- Requires `--payment-recipient` flag

### `x402_priority_signer`
- X402 payment with priority signer registration
- Registers payer as priority signer via HTTP API
- Sends transfer transaction with x402 payment proof
- Tests priority signer flow with payment verification
- Requires `--payment-recipient` flag

### `x402_bundle`
- X402 payment with bundle registration
- Creates bundle of transactions with x402 payment
- Registers bundle via HTTP API using transaction signatures
- Tests complete x402 bundle workflow
- Requires `--payment-recipient` flag

## 🔐 X402 Payment Protocol

The load tester supports three x402 payment flows:

### Payment Flow
1. **x402_payment**: Simple payment to recipient
   - Sends SOL transfer to payment recipient
   - No scheduler interaction
   - Tests basic payment verification

### Priority Signer Flow
1. **x402_priority_signer**: Payment + Priority Registration
   - Sends payment to recipient
   - Registers payer as priority signer via API
   - Sends transfer transaction
   - Tests priority signer scheduling

### Bundle Flow
1. **x402_bundle**: Payment + Bundle Registration
   - Sends payment to recipient  
   - Creates multiple transactions
   - Registers bundle via API using **transaction signatures** (first 32 bytes)
   - Submits all transactions
   - Tests complete bundle workflow with payment verification

**Important**: All x402 types use transaction signatures instead of message hashes for bundle identification.

## ⚙️ Performance

The load tester uses:
- **Async I/O** with Tokio for maximum concurrency
- **Streaming** results to minimize memory
- **Controlled buffer** to avoid RPC overload

Typical throughput:
- **Local validator**: 1000-5000 TPS
- **Devnet**: 50-200 TPS
- **Mainnet (public)**: 10-50 TPS
- **With --no-confirm**: 5000-10000+ TPS (local)

## 🐛 Troubleshooting

### Error: "Account not found"
```bash
# Make sure you have SOL
solana balance

# Request airdrop if needed
solana airdrop 2
```

### Error: "429 Too Many Requests"
- Reduce `--concurrency`
- Use private RPC endpoint
- Add delays between requests

### Low Performance
- Use `--transaction-type compute` for minimal overhead
- Increase `--concurrency` if RPC supports it
- Make sure you're using `--release` build
- Use `--no-confirm` for maximum speed

### Slow Compilation
```bash
# Use compilation cache
export CARGO_INCREMENTAL=1

# Build only what changed
cargo build --release --bin load-test
```

### Connection Timeouts
- Check network connectivity
- Verify RPC endpoint is accessible
- Try reducing concurrency
- Use local validator for testing

### Bundle Registration Failed (422)
- Ensure scheduler is running on `http://localhost:8080`
- For x402 bundles, payment signature is required
- Verify bundle API endpoint is accessible
- Check that transaction signatures are being used (not message hashes)

## 🎨 Features

- ✅ Multiple transaction types (airdrop, transfer, bundle_transfer, memo, compute, x402_payment, x402_priority_signer, x402_bundle)
- ✅ Configurable concurrency
- ✅ Detailed latency statistics
- ✅ Custom keypair support
- ✅ Dry-run mode
- ✅ No-confirmation mode for maximum throughput
- ✅ Real-time progress tracking
- ✅ Error categorization and reporting
- ✅ Bundle registration with scheduler API using transaction signatures
- ✅ X402 payment protocol support (payment, priority signer, bundle)
- ✅ Fully async implementation (no blocking I/O)

## 🤝 Contributing

This load tester is part of the `ext-scheduler-bindings` project and follows the same code conventions.

## 📄 License

Part of the ext-scheduler-bindings project. See root LICENSE file for details.

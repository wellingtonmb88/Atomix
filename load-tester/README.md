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
| `-t, --transaction-type <TYPE>` | Type: airdrop, transfer, memo, compute | `memo` |
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

### `memo`
- Transaction with memo program
- Minimal overhead
- Ideal for throughput testing

### `compute`
- Transaction with compute budget
- Minimal processing
- Maximum performance for benchmarks

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

## 📚 Additional Documentation

- See `../LOAD_TEST_README.md` for complete documentation
- See `../QUICK_START.md` for quick start guide
- Alternative scripts in Bash, Python and TypeScript available

## 🎨 Features

- ✅ Multiple transaction types (airdrop, transfer, memo, compute)
- ✅ Configurable concurrency
- ✅ Detailed latency statistics
- ✅ Custom keypair support
- ✅ Dry-run mode
- ✅ No-confirmation mode for maximum throughput
- ✅ Real-time progress tracking
- ✅ Error categorization and reporting
- ✅ Bundle registration support (for scheduler testing)

## 🤝 Contributing

This load tester is part of the `ext-scheduler-bindings` project and follows the same code conventions.

## 📄 License

Part of the ext-scheduler-bindings project. See root LICENSE file for details.

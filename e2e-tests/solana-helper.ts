/**
 * SolanaHelper - Simple wrapper around @solana/web3.js
 * Provides a cleaner API for common Solana operations
 */

import {
  Connection,
  Keypair,
  LAMPORTS_PER_SOL,
  PublicKey,
  sendAndConfirmTransaction,
  SystemProgram,
  Transaction,
} from "@solana/web3.js";

export interface SolanaHelperConfig {
  rpcUrl: string;
  keypair?: Keypair;
}

export class SolanaHelper {
  public connection: Connection;
  public wallet: Keypair;

  constructor(config: SolanaHelperConfig) {
    this.connection = new Connection(config.rpcUrl, "confirmed");
    this.wallet = config.keypair || Keypair.generate();
  }

  /**
   * Generate a new keypair
   */
  generateKeypair(): Keypair {
    return Keypair.generate();
  }

  /**
   * Get wallet balance in SOL
   */
  async getBalance(pubkey?: PublicKey): Promise<number> {
    const address = pubkey || this.wallet.publicKey;
    const lamports = await this.connection.getBalance(address);
    return lamports / LAMPORTS_PER_SOL;
  }

  /**
   * Request airdrop to wallet
   */
  async requestAirdrop(amount: number, pubkey?: PublicKey): Promise<string> {
    const address = pubkey || this.wallet.publicKey;
    const lamports = amount * LAMPORTS_PER_SOL;

    const signature = await this.connection.requestAirdrop(address, lamports);
    await this.connection.confirmTransaction(signature, "confirmed");

    return signature;
  }

  /**
   * Transfer SOL to another address
   */
  async transfer(
    to: PublicKey,
    amount: number,
    from?: Keypair,
  ): Promise<string> {
    const sender = from || this.wallet;
    const lamports = amount * LAMPORTS_PER_SOL;

    const transaction = new Transaction().add(
      SystemProgram.transfer({
        fromPubkey: sender.publicKey,
        toPubkey: to,
        lamports,
      }),
    );

    const signature = await sendAndConfirmTransaction(
      this.connection,
      transaction,
      [sender],
      { commitment: "confirmed" },
    );

    return signature;
  }

  /**
   * Send and confirm a transaction
   */
  async sendTransaction(
    transaction: Transaction,
    signers?: Keypair[],
  ): Promise<string> {
    const allSigners = signers ? [this.wallet, ...signers] : [this.wallet];

    const signature = await sendAndConfirmTransaction(
      this.connection,
      transaction,
      allSigners,
      { commitment: "confirmed" },
    );

    return signature;
  }

  /**
   * Get recent blockhash
   */
  async getRecentBlockhash(): Promise<string> {
    const { blockhash } = await this.connection.getLatestBlockhash();
    return blockhash;
  }
}

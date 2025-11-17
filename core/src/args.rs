use std::net::SocketAddr;
use std::path::PathBuf;

use clap::{Parser, ValueHint};

#[derive(Debug, Parser)]
#[command(version = toolbox::version!(), long_version = toolbox::long_version!())]
pub(crate) struct Args {
    /// Path to scheduler bindings ipc server.
    #[clap(long, value_hint = ValueHint::FilePath)]
    pub(crate) bindings_ipc: PathBuf,
    /// If provided, will write hourly log files to this directory.
    #[arg(long, value_hint = ValueHint::DirPath)]
    pub(crate) logs: Option<PathBuf>,
    /// HTTP API server address (e.g., 127.0.0.1:8080)
    #[arg(long, default_value = "127.0.0.1:8080")]
    pub(crate) http_addr: SocketAddr,
    /// Solana RPC URL for payment verification
    #[arg(long)]
    pub(crate) rpc_url: Option<String>,
    /// Payment recipient pubkey (required if rpc_url is provided)
    #[arg(long)]
    pub(crate) payment_recipient: Option<String>,
}

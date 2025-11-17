use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use futures::StreamExt;
use futures::stream::FuturesUnordered;
use solana_pubkey::Pubkey;
use tokio::runtime::Runtime;
use tokio::signal::unix::SignalKind;
use toolbox::shutdown::Shutdown;
use toolbox::tokio::NamedTask;
use tracing::{error, info, warn};

use crate::scheduler_thread::SchedulerThread;
use greedy_scheduler::api::SchedulerApi;
use http_api::PaymentVerifier;

pub(crate) struct ControlThread {
    shutdown: Shutdown,
    threads: FuturesUnordered<NamedTask<std::thread::Result<()>>>,
    pub api: SchedulerApi,
}

impl ControlThread {
    pub(crate) fn run_in_place(
        bindings_ipc: PathBuf,
        http_addr: SocketAddr,
        rpc_url: Option<String>,
        payment_recipient: Option<String>,
    ) -> std::thread::Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let server =
            ControlThread::setup(&runtime, bindings_ipc, http_addr, rpc_url, payment_recipient);

        runtime.block_on(server.run())
    }

    fn setup(
        runtime: &Runtime,
        bindings_ipc: PathBuf,
        http_addr: SocketAddr,
        rpc_url: Option<String>,
        payment_recipient: Option<String>,
    ) -> Self {
        let shutdown = Shutdown::new();

        // Create API for scheduler communication
        let (api, command_rx) = SchedulerApi::new();

        // Setup app threads.
        let scheduler_threads =
            vec![SchedulerThread::spawn(shutdown.clone(), bindings_ipc, command_rx)];

        // Use tokio to listen on all thread exits concurrently.
        let threads: FuturesUnordered<_> = scheduler_threads
            .into_iter()
            .map(|thread| {
                let name = thread.thread().name().unwrap().to_string();
                info!(name, "Thread spawned");

                NamedTask::new(runtime.spawn_blocking(move || thread.join()), name)
            })
            .collect();

        // Create payment verifier if RPC URL and recipient are provided
        let payment_verifier = match (rpc_url, payment_recipient) {
            (Some(url), Some(recipient_str)) => match Pubkey::from_str(&recipient_str) {
                Ok(recipient) => {
                    info!("Payment verification enabled with recipient: {}", recipient);
                    Some(PaymentVerifier::new(url, recipient))
                }
                Err(e) => {
                    error!("Invalid payment recipient pubkey: {}", e);
                    warn!("Payment verification DISABLED");
                    None
                }
            },
            (Some(_), None) => {
                warn!("RPC URL provided but no payment recipient - payment verification DISABLED");
                None
            }
            (None, Some(_)) => {
                warn!("Payment recipient provided but no RPC URL - payment verification DISABLED");
                None
            }
            (None, None) => {
                warn!("Payment verification DISABLED - no RPC URL or recipient provided");
                None
            }
        };

        // Spawn HTTP API server
        let api_clone = api.clone();
        let shutdown_clone = shutdown.clone();
        let http_task = NamedTask::new(
            runtime.spawn(async move {
                if let Err(e) =
                    http_api::start_server(api_clone, http_addr, shutdown_clone, payment_verifier)
                        .await
                {
                    error!("HTTP API server error: {}", e);
                }
                Ok(())
            }),
            "HTTP API".to_string(),
        );

        let threads = std::iter::once(http_task).chain(threads).collect();

        ControlThread { shutdown, threads, api }
    }

    async fn run(mut self) -> std::thread::Result<()> {
        let mut sigterm = tokio::signal::unix::signal(SignalKind::terminate()).unwrap();
        let mut sigint = tokio::signal::unix::signal(SignalKind::interrupt()).unwrap();

        let mut exit = tokio::select! {
            () = self.shutdown.cancelled() => Ok(()),

            _ = sigterm.recv() => {
                info!("SIGTERM caught, stopping server");

                Ok(())
            },
            _ = sigint.recv() => {
                info!("SIGINT caught, stopping server");

                Ok(())
            },
            opt = self.threads.next() => {
                let (name, res) = opt.unwrap();
                error!(%name, ?res, "Thread exited unexpectedly");

                res.unwrap().and_then(|()| Err(Box::new("Thread exited unexpectedly")))
            }
        };

        // Trigger shutdown.
        self.shutdown.shutdown();

        // Wait for all threads to exit, reporting the first error as the ultimate
        // error.
        while let Some((name, res)) = self.threads.next().await {
            info!(%name, ?res, "Thread exited");
            exit = exit.and(res.unwrap());
        }

        exit
    }
}

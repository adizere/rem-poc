mod storage;
mod client;
mod task;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use reth::api::EngineTypes;
use reth::providers::BlockReaderIdExt;
use reth_beacon_consensus::BeaconEngineMessage;
use reth_chainspec::ChainSpec;
use reth_transaction_pool::{TransactionPool};

use crate::chain::client::MalachiteClient;
use crate::chain::storage::Storage;
use crate::chain::task::MalachiteELTask;

type MalachiteChainDecision<Transaction> = Vec<Arc<Transaction>>;

/// Largely based on `AutoSealBuilder`
#[derive(Debug)]
pub struct MalachiteChainBuilder<Client, Pool, Engine: EngineTypes, EvmConfig> {
    client: Client,
    pool: Pool,
    storage: Storage,
    chain_spec: Arc<ChainSpec>,
    to_engine: UnboundedSender<BeaconEngineMessage<Engine>>,
    evm_config: EvmConfig,
}

// === impl MalachiteChainBuilder ===

impl<Client, Pool, Engine, EvmConfig> MalachiteChainBuilder<Client, Pool, Engine, EvmConfig>
where
    Client: BlockReaderIdExt,
    Pool: TransactionPool,
    Engine: EngineTypes,
{
    /// Creates a new builder instance to configure all parts.
    pub fn new(
        chain_spec: Arc<ChainSpec>,
        client: Client,
        pool: Pool,
        to_engine: UnboundedSender<BeaconEngineMessage<Engine>>,
        evm_config: EvmConfig,
    ) -> Self {
        let latest_header = client
            .latest_header()
            .ok()
            .flatten()
            .unwrap_or_else(|| chain_spec.sealed_genesis_header());

        Self {
            storage: Storage::new(latest_header),
            client,
            pool,
            chain_spec,
            to_engine,
            evm_config,
        }
    }

    /// Consumes the type and returns all components
    pub fn build(
        self,
    ) -> (MalachiteClient, MalachiteChain<Pool>, MalachiteELTask<Client, Pool, EvmConfig, Engine>) {
        let Self {
            client,
            pool,
            chain_spec,
            storage,
            to_engine,
            evm_config } = self;
        let auto_client = MalachiteClient::new(storage.clone());

        // Create a channel on which the Chain sends to the Task new decisions
        let (chain_tx, chain_rx) = unbounded_channel();

        // Instantiate the Malachite Chain here
        // todo adi introduce new type & spawn the task with the simulator
        let chain: MalachiteChain<Pool> = MalachiteChain::new(chain_tx);

        // This is the task being polled periodically to consume every new block that the
        // Malachite chain creates.
        let el_task = MalachiteELTask::new(
            Arc::clone(&chain_spec),
            to_engine,
            storage,
            client,
            pool,
            evm_config,
            chain_rx,
        );
        (auto_client, chain, el_task)
    }
}

/// Wrapper over the malachite simulator, which handles block finalization
pub struct MalachiteChain<Pool: TransactionPool> {
    to_task: UnboundedSender<MalachiteChainDecision<<Pool as TransactionPool>::Transaction>>,
}

impl<Pool: TransactionPool> MalachiteChain<Pool> {
    pub fn new(chain_tx: UnboundedSender<MalachiteChainDecision<<Pool as TransactionPool>::Transaction>>) -> Self
    {
        Self {
            to_task:chain_tx 
        }
    }
}

impl<Pool: TransactionPool> Future for MalachiteChain<Pool> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        use tokio::time::Duration;
        use std::thread::sleep;
        
        loop {
            sleep(Duration::from_secs(2));
            println!("\t\tjust chillin");
        }
    }
}
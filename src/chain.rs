mod application;
mod client;
mod common;
mod context;
mod decision;
mod simulator;
mod storage;
mod task;

use reth::api::EngineTypes;
use reth::providers::BlockReaderIdExt;
use reth_beacon_consensus::BeaconEngineMessage;
use reth_chainspec::ChainSpec;
use reth_primitives::{IntoRecoveredTransaction, TransactionSigned};
use reth_transaction_pool::TransactionPool;
use std::borrow::Borrow;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tracing::error;

use crate::chain::client::MalachiteClient;
use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::value::BaseValue;
use crate::chain::decision::DecisionStep;
use crate::chain::simulator::{DecisionStepReceiver, DecisionStepSender, Simulator};
use crate::chain::storage::Storage;
use crate::chain::task::MalachiteELTask;

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
    ) -> (
        MalachiteClient,
        MalachiteChain<Pool>,
        MalachiteChainConsumer,
        MalachiteELTask<Client, Pool, EvmConfig, Engine>,
    ) {
        let Self {
            client,
            pool,
            chain_spec,
            storage,
            to_engine,
            evm_config,
        } = self;
        let auto_client = MalachiteClient::new(storage.clone());

        // A channel on which the Chain decisions get consumed
        let (d_tx, d_rx) = unbounded_channel();

        // Create a channel on which the Chain sends to the Task new decisions
        let (chain_tx, chain_rx) = unbounded_channel();

        // Instantiate the Malachite Chain here
        // todo adi introduce new type & spawn the task with the simulator
        let chain: MalachiteChain<Pool> = MalachiteChain::new(pool.clone(), d_tx);

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

        let consumer = MalachiteChainConsumer::new(chain_tx, d_rx);

        (auto_client, chain, consumer, el_task)
    }
}

/// Wrapper over the malachite chain simulator, which handles block finalization
pub struct MalachiteChain<Pool: TransactionPool> {
    pool: Pool,
    to_consumer: DecisionStepSender,
}

impl<Pool: TransactionPool> MalachiteChain<Pool> {
    pub fn new(pool: Pool, d_tx: DecisionStepSender) -> Self {
        Self {
            pool,
            to_consumer: d_tx,
        }
    }
}

impl<Pool: TransactionPool + Unpin + 'static> Future for MalachiteChain<Pool> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        // Create a network of 4 peers
        let (mut n, mut states, proposals) = Simulator::new(4, this.to_consumer.clone());
        let pool = this.pool.clone();

        // Spawn the task that produces values to be proposed -- the block builder
        // todo adi: fix this hack by creating instead a `spawn_critical_blocking` block
        tokio::spawn(async move {
            // Unique identifier for every value that Malachite attempts to finalize
            let mut value_counter = 0;

            loop {
                // Fetch the currently available transactions if any
                // The chain will use each set of transactions as consensus proposals
                let txs: Vec<_> = pool
                    .best_transactions()
                    .map(|t| t.to_recovered_transaction().into_signed())
                    .collect();
                println!(
                    "block builder: block ready id={}, tx count={}",
                    value_counter,
                    txs.len()
                );

                let value = BaseValue {
                    id: value_counter,
                    transactions: txs,
                };

                proposals
                    .send(value)
                    .expect("could not send new value to be proposed");

                value_counter += 1;
            }
        });

        // Run the chain simulator system
        // Blocking method, starts the simulated network and handles orchestration of
        // block building
        n.run(&mut states);

        Poll::Pending
    }
}

pub struct MalachiteChainConsumer {
    to_task: UnboundedSender<Vec<TransactionSigned>>,
    from_chain: DecisionStepReceiver,
}

impl MalachiteChainConsumer {
    pub fn new(
        chain_tx: UnboundedSender<Vec<TransactionSigned>>,
        d_rx: DecisionStepReceiver,
    ) -> Self {
        Self {
            to_task: chain_tx,
            from_chain: d_rx,
        }
    }

    pub async fn run(mut self) {
        let mut decisions = HashMap::new();

        // Busy loop, simply consume the decided heights
        loop {
            let res = self.from_chain.recv().await;
            match res {
                Some(d) => {
                    match d {
                        DecisionStep::Proposed(p) => {
                            // Only insert once, index by height
                            // Consider alternative indexing by p.value_id
                            if !decisions.contains_key(p.height.borrow()) {
                                println!("chain consumer: block with id {} was used as a proposal at height {}", p.value.id, p.height);
                                decisions.insert(p.height, p.value);
                            }
                        }
                        DecisionStep::Finalized(f) if f.peer.eq(&BasePeerAddress(0)) => {
                            // Assumption: Consume only decisions that peer 0 has finalized
                            let value = decisions.get(&f.height).unwrap();
                            let txs = value.transactions.clone();

                            println!("chain consumer: finalized height # {} with block id {} and count={}, relaying to the execution task",
                                             f.height, f.value_id, txs.len());

                            // relay the newly produced value to the task
                            self.to_task
                                .send(txs)
                                .expect("unable to relay the newly produced block");
                        }
                        _ => {
                            // some other peer than (0) finalized, not relaying to the task
                        }
                    }
                }
                None => {
                    error!("error receiving decisions");
                    error!("stopping");
                    break;
                }
            }
        }
    }
}

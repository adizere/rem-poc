use crate::chain::storage::Storage;
use crate::chain::MalachiteChainDecision;
use futures_util::{future::BoxFuture, FutureExt};
use reth_auto_seal_consensus::MiningMode;
use reth_beacon_consensus::{BeaconEngineMessage, ForkchoiceStatus};
use reth_chainspec::ChainSpec;
use reth_engine_primitives::EngineTypes;
use reth_evm::execute::BlockExecutorProvider;
use reth_primitives::IntoRecoveredTransaction;
use reth_provider::{CanonChainTracker, StateProviderFactory};
use reth_rpc_types::engine::ForkchoiceState;
use reth_stages_api::PipelineEvent;
use reth_tokio_util::EventStream;
use reth_transaction_pool::{TransactionPool, ValidPoolTransaction};
use std::{
    collections::VecDeque,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::{mpsc::UnboundedSender, oneshot};
use tracing::{debug, error, warn};

/// Patterned after `MiningTask`
///
/// A Future that listens for new ready transactions and puts new blocks into storage
///
/// Chain ----->--->----|----> Task <----> BeaconConsensusEngine
///  {consensus client} | {execution client}
pub struct MalachiteELTask<Client, Pool: TransactionPool, Executor, Engine: EngineTypes> {
    /// The configured chain spec
    chain_spec: Arc<ChainSpec>,
    /// The client used to interact with the state
    client: Client,
    /// Single active future that inserts a new block into `storage`
    insert_task: Option<BoxFuture<'static, Option<EventStream<PipelineEvent>>>>,
    /// Shared storage to insert new blocks
    storage: Storage,
    /// Pool where transactions are stored
    pool: Pool,
    /// backlog of sets of transactions ready to be mined
    queued: VecDeque<Vec<Arc<ValidPoolTransaction<<Pool as TransactionPool>::Transaction>>>>,
    // TODO: ideally this would just be a sender of hashes
    to_engine: UnboundedSender<BeaconEngineMessage<Engine>>,
    /// The pipeline events to listen on
    pipe_line_events: Option<EventStream<PipelineEvent>>,
    /// The type used for block execution
    block_executor: Executor,
    /// The legacy miner is the AutoSeal-style miner that produces blocks without consensus
    /// todo adi: This will be removed ASAP; keeping just for testing
    legacy_miner: MiningMode,
    /// The new miner is the Malachite chain simulator that produces blocks using local consensus engine
    chain_rx: UnboundedReceiver<MalachiteChainDecision<<Pool as TransactionPool>::Transaction>>,
}

impl<Executor, Client, Pool: TransactionPool, Engine: EngineTypes>
    MalachiteELTask<Client, Pool, Executor, Engine>
{
    /// Creates the task
    pub fn new(
        chain_spec: Arc<ChainSpec>,
        to_engine: UnboundedSender<BeaconEngineMessage<Engine>>,
        storage: Storage,
        client: Client,
        pool: Pool,
        block_executor: Executor,
        chain_rx: UnboundedReceiver<MalachiteChainDecision<<Pool as TransactionPool>::Transaction>>,
    ) -> Self {
        let mode = MiningMode::interval(std::time::Duration::from_secs(1));
        Self {
            chain_spec,
            client,
            insert_task: None,
            storage,
            pool,
            to_engine,
            queued: Default::default(),
            pipe_line_events: None,
            block_executor,
            legacy_miner: mode,
            chain_rx,
        }
    }

    /// Sets the pipeline events to listen on.
    pub fn set_pipeline_events(&mut self, events: EventStream<PipelineEvent>) {
        self.pipe_line_events = Some(events);
    }
}

impl<Executor, Client, Pool, Engine> Future for MalachiteELTask<Client, Pool, Executor, Engine>
where
    Client: StateProviderFactory + CanonChainTracker + Clone + Unpin + 'static,
    Pool: TransactionPool + Unpin + 'static,
    Engine: EngineTypes,
    Executor: BlockExecutorProvider,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        println!("\t 0. inside poll of MalachiteTask");
        // let id = cx.

        loop {
            //
            // todo adi: cleanup this
            //  the miner is no longer needed most likely
            if let Poll::Ready(transactions) = this.legacy_miner.poll(&this.pool, cx) {
                // miner returned a set of transaction that we feed to the producer
                println!("\t 1. some ready txes {:?}", transactions);
                this.queued.push_back(transactions);
            }

            if this.insert_task.is_none() {
                // todo adi: cleanup this
                //  we should be finalizing blocks regularly
                if this.queued.is_empty() {
                    // nothing to insert
                    println!("\t 2. (A) nothing to insert");
                    break;
                }

                let storage = this.storage.clone();
                // todo adi: we'll function without user transactions in a first try
                let transactions = this.queued.pop_front().expect("not empty");

                let to_engine = this.to_engine.clone();
                let client = this.client.clone();
                let chain_spec = Arc::clone(&this.chain_spec);
                let pool = this.pool.clone();
                let events = this.pipe_line_events.take();
                let executor = this.block_executor.clone();

                println!("\t 2. (B) SOMETHING to insert");

                // Create the mining future that creates a block, notifies the engine that drives
                // the pipeline
                this.insert_task = Some(Box::pin(async move {
                    let mut storage = storage.write().await;

                    let transactions: Vec<_> = transactions
                        .into_iter()
                        .map(|tx| {
                            let recovered = tx.to_recovered_transaction();
                            recovered.into_signed()
                        })
                        .collect();
                    let ommers = vec![];

                    println!(
                        "\t 3. inside insert_task, ready to build & exec txes {:?}",
                        transactions
                    );

                    match storage.build_and_execute(
                        transactions.clone(),
                        ommers.clone(),
                        &client,
                        chain_spec,
                        &executor,
                    ) {
                        Ok((new_header, _bundle_state)) => {
                            // clear all transactions from pool
                            pool.remove_transactions(
                                transactions.iter().map(|tx| tx.hash()).collect(),
                            );

                            let state = ForkchoiceState {
                                head_block_hash: new_header.hash(),
                                finalized_block_hash: new_header.hash(),
                                safe_block_hash: new_header.hash(),
                            };
                            drop(storage);

                            // TODO: make this a future
                            // await the fcu call rx for SYNCING, then wait for a VALID response
                            loop {
                                // send the new update to the engine, this will trigger the engine
                                // to download and execute the block we just inserted
                                let (tx, rx) = oneshot::channel();
                                let _ = to_engine.send(BeaconEngineMessage::ForkchoiceUpdated {
                                    state,
                                    payload_attrs: None,
                                    tx,
                                });
                                debug!(target: "consensus::auto", ?state, "Sent fork choice update");

                                println!(
                                    "\t 4. inside inner loop: Sent fork choice update {:?}",
                                    state
                                );

                                // response from execution client
                                match rx.await.unwrap() {
                                    Ok(fcu_response) => {
                                        println!(
                                            "4 (b) -- .await.unwrap() returned Ok {:?}",
                                            fcu_response
                                        );
                                        match fcu_response.forkchoice_status() {
                                            ForkchoiceStatus::Valid => break,
                                            ForkchoiceStatus::Invalid => {
                                                error!(target: "consensus::auto", ?fcu_response, "Forkchoice update returned invalid response");
                                                return None;
                                            }
                                            ForkchoiceStatus::Syncing => {
                                                debug!(target: "consensus::auto", ?fcu_response, "Forkchoice update returned SYNCING, waiting for VALID");
                                                // wait for the next fork choice update
                                                continue;
                                            }
                                        }
                                    }
                                    Err(err) => {
                                        println!("4 (b) -- .await.unwrap() returned Err");
                                        error!(target: "consensus::auto", %err, "Autoseal fork choice update failed");
                                        return None;
                                    }
                                }
                            }

                            println!("4 (c) -- exited the inner loop");

                            // update canon chain for rpc
                            client.set_canonical_head(new_header.clone());
                            client.set_safe(new_header.clone());
                            client.set_finalized(new_header.clone());
                        }
                        Err(err) => {
                            warn!(target: "consensus::auto", %err, "failed to execute block")
                        }
                    }

                    println!("\t 5. storage finished build_and_execute(): end of insert task: produced events {:?}", events);

                    events
                }));
            }

            if let Some(mut fut) = this.insert_task.take() {
                println!("\t 6. PRE-POLL. took insert task");
                match fut.poll_unpin(cx) {
                    Poll::Ready(events) => {
                        println!("\t 6. POST-POLL returned Ready {:?}", events);
                        this.pipe_line_events = events;
                    }
                    Poll::Pending => {
                        this.insert_task = Some(fut);
                        println!("\t 6. POST-POLL returned Pending");
                        break;
                    }
                }
            }
        }
        println!("\t 7. end poll for task");

        Poll::Pending
    }
}

impl<Client, Pool: TransactionPool, EvmConfig: std::fmt::Debug, Engine: EngineTypes> std::fmt::Debug
    for MalachiteELTask<Client, Pool, EvmConfig, Engine>
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MalachiteTask").finish_non_exhaustive()
    }
}

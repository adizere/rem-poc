use crate::chain::storage::Storage;
use futures_util::{future::BoxFuture, FutureExt};
use reth_beacon_consensus::{BeaconEngineMessage, ForkchoiceStatus};
use reth_chainspec::ChainSpec;
use reth_engine_primitives::EngineTypes;
use reth_evm::execute::BlockExecutorProvider;
use reth_primitives::TransactionSigned;
use reth_provider::{CanonChainTracker, StateProviderFactory};
use reth_rpc_types::engine::ForkchoiceState;
use reth_stages_api::PipelineEvent;
use reth_tokio_util::EventStream;
use reth_transaction_pool::TransactionPool;
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
/// Chain ----->--->---> Consumer --->--|----> Task <----> BeaconConsensusEngine
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
    queued: VecDeque<Vec<TransactionSigned>>,
    // TODO: ideally this would just be a sender of hashes
    to_engine: UnboundedSender<BeaconEngineMessage<Engine>>,
    /// The pipeline events to listen on
    pipe_line_events: Option<EventStream<PipelineEvent>>,
    /// The type used for block execution
    block_executor: Executor,
    // The legacy miner is the AutoSeal-style miner that produces blocks without consensus
    // Adi: This has been deprecated in this proof of concept, because we're producing blocks
    // using Malachite simulator.
    // https://github.com/paradigmxyz/reth/blob/f25367cffdd6a181e872deedf21e7c0ff0dc0e44/crates/consensus/auto-seal/src/task.rs#L97
    // _legacy_miner: MiningMode,
    /// The Malachite chain simulator that produces blocks using a local consensus engine
    chain_rx: UnboundedReceiver<Vec<TransactionSigned>>,
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
        chain_rx: UnboundedReceiver<Vec<TransactionSigned>>,
    ) -> Self {
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

        loop {
            if let Poll::Ready(v) = this.chain_rx.poll_recv(cx) {
                let v = v.unwrap();
                this.queued.push_back(v);
            }

            if this.insert_task.is_none() {
                // todo adi: cleanup this
                //  we should be finalizing blocks regularly
                if this.queued.is_empty() {
                    // nothing to insert
                    break;
                }

                let storage = this.storage.clone();
                // todo adi: we'll function without user transactions in a first try
                let transactions = this.queued.pop_front().expect("not empty");
                println!(
                    "task: executing {} transactions from the new block",
                    transactions.len()
                );
                // adi note:
                // the malachite (consensus) block is already finalized here
                // in a real integration consider finalization <> execution interlacing

                let to_engine = this.to_engine.clone();
                let client = this.client.clone();
                let chain_spec = Arc::clone(&this.chain_spec);
                let pool = this.pool.clone();
                let events = this.pipe_line_events.take();
                let executor = this.block_executor.clone();

                // Create the mining future that creates a block, notifies the engine that drives
                // the pipeline
                this.insert_task = Some(Box::pin(async move {
                    let mut storage = storage.write().await;

                    let ommers = vec![];

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

                                // println!(
                                //     "\t 4. inside inner loop: Sent fork choice update {:?}",
                                //     state
                                // );

                                // response from execution client
                                match rx.await.unwrap() {
                                    Ok(fcu_response) => {
                                        // println!(
                                        //     "4 (b) -- .await.unwrap() returned Ok {:?}",
                                        //     fcu_response
                                        // );
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
                                        // println!("4 (b) -- .await.unwrap() returned Err");
                                        error!(target: "consensus::auto", %err, "Autoseal fork choice update failed");
                                        return None;
                                    }
                                }
                            }

                            // println!("4 (c) -- exited the inner loop");

                            // update canon chain for rpc
                            client.set_canonical_head(new_header.clone());
                            client.set_safe(new_header.clone());
                            client.set_finalized(new_header.clone());
                        }
                        Err(err) => {
                            warn!(target: "consensus::auto", %err, "failed to execute block")
                        }
                    }

                    // println!("\t 5. storage finished build_and_execute(): end of insert task: produced events {:?}", events);

                    events
                }));
            }

            if let Some(mut fut) = this.insert_task.take() {
                // println!("\t 6. PRE-POLL. took insert task");
                match fut.poll_unpin(cx) {
                    Poll::Ready(events) => {
                        // println!("\t 6. POST-POLL returned Ready {:?}", events);
                        this.pipe_line_events = events;
                    }
                    Poll::Pending => {
                        this.insert_task = Some(fut);
                        // println!("\t 6. POST-POLL returned Pending");
                        break;
                    }
                }
            }
        }
        // println!("\t 7. end poll for task");

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

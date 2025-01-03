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
use std::time::Duration;
use reth::primitives::hex;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};
use tokio::time::sleep;
use tracing::error;

use crate::chain::client::MalachiteClient;
use crate::chain::context::address::BasePeerAddress;
use crate::chain::context::height::BaseHeight;
use crate::chain::context::value::BaseValue;
use crate::chain::decision::DecisionStep;
use crate::chain::simulator::Simulator;
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

        // Create a channel on which the Chain sends to the Task new decisions
        let (chain_tx, chain_rx) = unbounded_channel();

        println!("\t\t special case trial at bootstrap");
        let tx_bytes = hex!("02f872018307910d808507204d2cb1827d0094388c818ca8b9251b393131c08a736a67ccb19297880320d04823e2701c80c001a0cf024f4815304df2867a1a74e9d2707b6abda0337d2d54a4438d453f4160f190a07ac0e6b3bc9395b5b9c8b9e6d77204a236577a5b18467b9175c01de4faa208d9");
        let decoded = TransactionSigned::decode_enveloped(&mut &tx_bytes[..]).unwrap();
        let try_2 = vec![decoded];

        chain_tx.send(try_2).expect("panic");

        // Instantiate the Malachite Chain here
        // todo adi introduce new type & spawn the task with the simulator
        let chain: MalachiteChain<Pool> = MalachiteChain::new(pool.clone(), chain_tx.clone());

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

        let consumer = MalachiteChainConsumer::new(chain_tx);

        (auto_client, chain, consumer, el_task)
    }
}

/// Wrapper over the malachite chain simulator, which handles block finalization
pub struct MalachiteChain<Pool: TransactionPool> {
    to_task: UnboundedSender<Vec<TransactionSigned>>,
    pool: Pool,
    sleeper: tokio::time::Interval,
    counter: usize
}

impl<Pool: TransactionPool> MalachiteChain<Pool> {
    pub fn new(pool: Pool, chain_tx: UnboundedSender<Vec<TransactionSigned>>) -> Self {
        Self {
            to_task: chain_tx,
            pool,
            sleeper: tokio::time::interval(Duration::from_secs(1)),
            counter: 4,
        }
    }
}

impl<Pool: TransactionPool + std::marker::Unpin> Future for MalachiteChain<Pool> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let tx_bytes = hex!("02f872018307910d808507204d2cb1827d0094388c818ca8b9251b393131c08a736a67ccb19297880320d04823e2701c80c001a0cf024f4815304df2867a1a74e9d2707b6abda0337d2d54a4438d453f4160f190a07ac0e6b3bc9395b5b9c8b9e6d77204a236577a5b18467b9175c01de4faa208d9");
        let decoded = TransactionSigned::decode_enveloped(&mut &tx_bytes[..]).unwrap();

        loop {
            // if this.sleeper.poll_tick(_cx) == Poll::Pending {
            //     break;
            // }

            let vtry = vec![decoded.clone(); this.counter];
            assert_eq!(vtry.len(), this.counter);

            let p = this.to_task.send(vtry);
            println!("\t\t counter={} sent res={:?}", this.counter, p);

            this.counter += 1;
            if this.counter > 10 {
                return Poll::Ready(())
            }
        }

        Poll::Pending

        // Create a network of 4 peers
        // let (mut n, mut states, proposals, decision_steps) = Simulator::new(4);
        // let pool = self.pool.clone();
        // let to_task = self.to_task.clone();
        //
        // // Spawn the task that produces values to be proposed
        // tokio::spawn(async move {
        //     // Unique identifier for every value that Malachite attempts to finalize
        //     let mut value_counter = 0;
        //
        //     loop {
        //         // Fetch the currently available transactions if any
        //         // The chain will consume each set of transactions as consensus proposals
        //         let txs: Vec<_> = pool
        //             .best_transactions()
        //             .map(|t| t.to_recovered_transaction().into_signed())
        //             .collect();
        //         println!(
        //             "id={}, set size={} of transactions ready to become a proposal",
        //             value_counter,
        //             txs.len()
        //         );
        //
        //         let value = BaseValue {
        //             id: value_counter,
        //             transactions: txs,
        //         };
        //
        //         proposals
        //             .send(value)
        //             .expect("could not send new value to be proposed");
        //
        //         value_counter += 1;
        //     }
        // });

        // Spawn a thread in the background that handles decided values
        // tokio::spawn(async move {
        //     let mut decisions = HashMap::new();
        //
        //     // Busy loop, simply consume the decided heights
        //     loop {
        //         let res = decision_steps.recv();
        //         match res {
        //             Ok(d) => {
        //                 match d {
        //                     DecisionStep::Proposed(p) => {
        //                         // Only insert once, index by height
        //                         // Consider alternative indexing by p.value_id
        //                         if !decisions.contains_key(p.height.borrow()) {
        //                             println!("new proposal took place for height {}", p.height);
        //                             decisions.insert(p.height, p.value);
        //                         }
        //                     }
        //                     DecisionStep::Finalized(f) if f.peer.eq(&BasePeerAddress(0)) => {
        //                         // Assumption: Consume only decisions that peer 0 has finalized
        //                         let value = decisions.get(&f.height).unwrap();
        //                         let txs = value.transactions.clone();
        //
        //                         println!("peer 0 finalized proposal height # {} with id {} and count={}, relaying to the task",
        //                                  f.height, f.value_id, txs.len());
        //
        //                         // relay the newly produced value to the task
        //                         to_task
        //                             .send(txs)
        //                             .expect("unable to relay the newly produced block");
        //
        //                         if f.height == BaseHeight(2) {
        //                             println!("\t\t special case trial for height 2");
        //                             let tx_bytes = hex!("02f872018307910d808507204d2cb1827d0094388c818ca8b9251b393131c08a736a67ccb19297880320d04823e2701c80c001a0cf024f4815304df2867a1a74e9d2707b6abda0337d2d54a4438d453f4160f190a07ac0e6b3bc9395b5b9c8b9e6d77204a236577a5b18467b9175c01de4faa208d9");
        //                             let decoded = TransactionSigned::decode_enveloped(&mut &tx_bytes[..]).unwrap();
        //                             let try_2 = vec![decoded];
        //
        //                             to_task.send(try_2).expect("panic");
        //                         }
        //                     }
        //                     _ => {
        //                         // some other peer than (0) finalized, not relaying to the task
        //                     }
        //                 }
        //             }
        //             Err(err) => {
        //                 error!(error = ?err, "error receiving decisions");
        //                 error!("stopping");
        //                 break;
        //             }
        //         }
        //     }
        // });

        // Run the chain simulator system
        // Blocking method, starts the simulated network and handles orchestration of
        // block building
        // n.run(&mut states);
        //
        // Poll::Pending
    }
}

pub struct MalachiteChainConsumer {
    to_task: UnboundedSender<Vec<TransactionSigned>>,
    sleeper: tokio::time::Interval,
}

impl MalachiteChainConsumer {
    pub fn new(chain_tx: UnboundedSender<Vec<TransactionSigned>>) -> Self {
        Self {
            to_task: chain_tx,
            sleeper: tokio::time::interval(Duration::from_secs(1)),
        }
    }
}

impl Future for MalachiteChainConsumer {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.get_mut();

        let tx_bytes = hex!("02f872018307910d808507204d2cb1827d0094388c818ca8b9251b393131c08a736a67ccb19297880320d04823e2701c80c001a0cf024f4815304df2867a1a74e9d2707b6abda0337d2d54a4438d453f4160f190a07ac0e6b3bc9395b5b9c8b9e6d77204a236577a5b18467b9175c01de4faa208d9");
        let decoded = TransactionSigned::decode_enveloped(&mut &tx_bytes[..]).unwrap();
        let counter = 2;

        loop {
            if this.sleeper.poll_tick(cx) == Poll::Pending {
                break;
            }

            let vtry = vec![decoded.clone(); counter];
            assert_eq!(vtry.len(), counter);

            let p = this.to_task.send(vtry);
            println!("\t\t counter={} sent res={:?}", counter, p);
        }

        Poll::Pending

        // let mut decisions = HashMap::new();

        // Busy loop, simply consume the decided heights
        // loop {
        //     let res = decision_steps.recv();
        //     match res {
        //         Ok(d) => {
        //             match d {
        //                 DecisionStep::Proposed(p) => {
        //                     // Only insert once, index by height
        //                     // Consider alternative indexing by p.value_id
        //                     if !decisions.contains_key(p.height.borrow()) {
        //                         println!("new proposal took place for height {}", p.height);
        //                         decisions.insert(p.height, p.value);
        //                     }
        //                 }
        //                 DecisionStep::Finalized(f) if f.peer.eq(&BasePeerAddress(0)) => {
        //                     // Assumption: Consume only decisions that peer 0 has finalized
        //                     let value = decisions.get(&f.height).unwrap();
        //                     let txs = value.transactions.clone();
        //
        //                     println!("peer 0 finalized proposal height # {} with id {} and count={}, relaying to the task",
        //                              f.height, f.value_id, txs.len());
        //
        //                     // relay the newly produced value to the task
        //                     self.to_task
        //                         .send(txs)
        //                         .expect("unable to relay the newly produced block");
        //
        //                     if f.height == BaseHeight(2) {
        //                         println!("\t\t special case trial for height 2");
        //                         let tx_bytes = hex!("02f872018307910d808507204d2cb1827d0094388c818ca8b9251b393131c08a736a67ccb19297880320d04823e2701c80c001a0cf024f4815304df2867a1a74e9d2707b6abda0337d2d54a4438d453f4160f190a07ac0e6b3bc9395b5b9c8b9e6d77204a236577a5b18467b9175c01de4faa208d9");
        //                         let decoded = TransactionSigned::decode_enveloped(&mut &tx_bytes[..]).unwrap();
        //                         let try_2 = vec![decoded];
        //
        //                         to_task.send(try_2).expect("panic");
        //                     }
        //                 }
        //                 _ => {
        //                     // some other peer than (0) finalized, not relaying to the task
        //                 }
        //             }
        //         }
        //         Err(err) => {
        //             error!(error = ?err, "error receiving decisions");
        //             error!("stopping");
        //             break;
        //         }
        //     }
        // }
    }
}
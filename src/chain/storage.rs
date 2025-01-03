use reth::primitives::{
    eip4844::calculate_excess_blob_gas, proofs, Block, BlockBody, BlockHash, BlockHashOrNumber,
    BlockNumber, Bloom, Header, Requests, SealedHeader, TransactionSigned, Withdrawals, B256, U256,
};
use reth_chainspec::{ChainSpec, EthereumHardforks};
use reth_evm::execute::{BlockExecutionOutput, BlockExecutorProvider, Executor};
use reth_execution_errors::{
    BlockExecutionError, BlockValidationError, InternalBlockExecutionError,
};
use reth_execution_types::ExecutionOutcome;
use reth_provider::{StateProviderFactory, StateRootProvider};
use reth_revm::database::StateProviderDatabase;
use reth_tracing::tracing::trace;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

/// In memory storage for the Malachite chain
/// Largely based on `Storage` from reth `AutoSealConsensus`
#[derive(Debug, Clone, Default)]
pub(crate) struct Storage {
    inner: Arc<RwLock<StorageInner>>,
}

impl Storage {
    /// Initializes the [Storage] with the given best block. This should be initialized with the
    /// highest block in the chain, if there is a chain already stored on-disk.
    pub(crate) fn new(best_block: SealedHeader) -> Self {
        let (header, best_hash) = best_block.split();
        let mut storage = StorageInner {
            best_hash,
            total_difficulty: header.difficulty,
            best_block: header.number,
            ..Default::default()
        };
        storage.headers.insert(header.number, header);
        storage.bodies.insert(best_hash, BlockBody::default());
        Self {
            inner: Arc::new(RwLock::new(storage)),
        }
    }

    /// Returns the write lock of the storage
    pub(crate) async fn write(&self) -> RwLockWriteGuard<'_, StorageInner> {
        self.inner.write().await
    }

    /// Returns the read lock of the storage
    pub(crate) async fn read(&self) -> RwLockReadGuard<'_, StorageInner> {
        self.inner.read().await
    }
}

/// In-memory storage for the chain the Malachite engine is building
#[derive(Default, Debug)]
pub(crate) struct StorageInner {
    /// Headers buffered for download.
    pub(crate) headers: HashMap<BlockNumber, Header>,
    /// A mapping between block hash and number.
    pub(crate) hash_to_number: HashMap<BlockHash, BlockNumber>,
    /// Bodies buffered for download.
    pub(crate) bodies: HashMap<BlockHash, BlockBody>,
    /// Tracks best block
    pub(crate) best_block: u64,
    /// Tracks hash of best block
    pub(crate) best_hash: B256,
    /// The total difficulty of the chain until this block
    pub(crate) total_difficulty: U256,
}

// === impl StorageInner ===

impl StorageInner {
    /// Returns the block hash for the given block number if it exists.
    pub(crate) fn block_hash(&self, num: u64) -> Option<BlockHash> {
        self.hash_to_number
            .iter()
            .find_map(|(k, v)| num.eq(v).then_some(*k))
    }

    /// Returns the matching header if it exists.
    pub(crate) fn header_by_hash_or_number(
        &self,
        hash_or_num: BlockHashOrNumber,
    ) -> Option<Header> {
        let num = match hash_or_num {
            BlockHashOrNumber::Hash(hash) => self.hash_to_number.get(&hash).copied()?,
            BlockHashOrNumber::Number(num) => num,
        };
        self.headers.get(&num).cloned()
    }

    /// Inserts a new header+body pair
    pub(crate) fn insert_new_block(&mut self, mut header: Header, body: BlockBody) {
        header.number = self.best_block + 1;
        header.parent_hash = self.best_hash;

        self.best_hash = header.hash_slow();
        self.best_block = header.number;
        self.total_difficulty += header.difficulty;

        trace!(target: "consensus::auto", num=self.best_block, hash=?self.best_hash, "inserting new block");
        self.headers.insert(header.number, header);
        self.bodies.insert(self.best_hash, body);
        self.hash_to_number.insert(self.best_hash, self.best_block);
    }

    /// Fills in pre-execution header fields based on the current best block and given
    /// transactions.
    pub(crate) fn build_header_template(
        &self,
        timestamp: u64,
        transactions: &[TransactionSigned],
        ommers: &[Header],
        withdrawals: Option<&Withdrawals>,
        requests: Option<&Requests>,
        chain_spec: &ChainSpec,
    ) -> Header {
        // check previous block for base fee
        let base_fee_per_gas = self.headers.get(&self.best_block).and_then(|parent| {
            parent.next_block_base_fee(chain_spec.base_fee_params_at_timestamp(timestamp))
        });

        let blob_gas_used = if chain_spec.is_cancun_active_at_timestamp(timestamp) {
            let mut sum_blob_gas_used = 0;
            for tx in transactions {
                if let Some(blob_tx) = tx.transaction.as_eip4844() {
                    sum_blob_gas_used += blob_tx.blob_gas();
                }
            }
            Some(sum_blob_gas_used)
        } else {
            None
        };

        let mut header = Header {
            parent_hash: self.best_hash,
            ommers_hash: proofs::calculate_ommers_root(ommers),
            beneficiary: Default::default(),
            state_root: Default::default(),
            transactions_root: proofs::calculate_transaction_root(transactions),
            receipts_root: Default::default(),
            withdrawals_root: withdrawals.map(|w| proofs::calculate_withdrawals_root(w)),
            logs_bloom: Default::default(),
            difficulty: U256::from(2),
            number: self.best_block + 1,
            gas_limit: chain_spec.max_gas_limit,
            gas_used: 0,
            timestamp,
            mix_hash: Default::default(),
            nonce: 0,
            base_fee_per_gas,
            blob_gas_used,
            excess_blob_gas: None,
            extra_data: Default::default(),
            parent_beacon_block_root: None,
            requests_root: requests.map(|r| proofs::calculate_requests_root(&r.0)),
        };

        if chain_spec.is_cancun_active_at_timestamp(timestamp) {
            let parent = self.headers.get(&self.best_block);
            header.parent_beacon_block_root =
                parent.and_then(|parent| parent.parent_beacon_block_root);
            header.blob_gas_used = Some(0);

            let (parent_excess_blob_gas, parent_blob_gas_used) = match parent {
                Some(parent_block)
                    if chain_spec.is_cancun_active_at_timestamp(parent_block.timestamp) =>
                {
                    (
                        parent_block.excess_blob_gas.unwrap_or_default(),
                        parent_block.blob_gas_used.unwrap_or_default(),
                    )
                }
                _ => (0, 0),
            };
            header.excess_blob_gas = Some(calculate_excess_blob_gas(
                parent_excess_blob_gas,
                parent_blob_gas_used,
            ))
        }

        header
    }

    /// Builds and executes a new block with the given transactions, on the provided executor.
    ///
    /// This returns the header of the executed block, as well as the poststate from execution.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn build_and_execute<Provider, Executor>(
        &mut self,
        transactions: Vec<TransactionSigned>,
        ommers: Vec<Header>,
        provider: &Provider,
        chain_spec: Arc<ChainSpec>,
        executor: &Executor,
    ) -> Result<(SealedHeader, ExecutionOutcome), BlockExecutionError>
    where
        Executor: BlockExecutorProvider,
        Provider: StateProviderFactory,
    {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        // if shanghai is active, include empty withdrawals
        let withdrawals = chain_spec
            .is_shanghai_active_at_timestamp(timestamp)
            .then_some(Withdrawals::default());
        // if prague is active, include empty requests
        let requests = chain_spec
            .is_prague_active_at_timestamp(timestamp)
            .then_some(Requests::default());

        let header = self.build_header_template(
            timestamp,
            &transactions,
            &ommers,
            withdrawals.as_ref(),
            requests.as_ref(),
            &chain_spec,
        );

        let block = Block {
            header,
            body: transactions,
            ommers: ommers.clone(),
            withdrawals: withdrawals.clone(),
            requests: requests.clone(),
        }
        .with_recovered_senders()
        .ok_or(BlockExecutionError::Validation(
            BlockValidationError::SenderRecoveryError,
        ))?;

        trace!(target: "consensus::auto", transactions=?&block.body, "executing transactions");

        let mut db = StateProviderDatabase::new(
            provider
                .latest()
                .map_err(InternalBlockExecutionError::LatestBlock)?,
        );

        // execute the block
        let BlockExecutionOutput {
            state,
            receipts,
            requests: block_execution_requests,
            gas_used,
            ..
        } = executor
            .executor(&mut db)
            .execute((&block, U256::ZERO).into())?;
        let execution_outcome = ExecutionOutcome::new(
            state,
            receipts.into(),
            block.number,
            vec![block_execution_requests.into()],
        );

        // todo(onbjerg): we should not pass requests around as this is building a block, which
        // means we need to extract the requests from the execution output and compute the requests
        // root here

        let Block {
            mut header, body, ..
        } = block.block;
        let body = BlockBody {
            transactions: body,
            ommers,
            withdrawals,
            requests,
        };

        trace!(target: "consensus::auto", ?execution_outcome, ?header, ?body, "executed block, calculating state root and completing header");

        // now we need to update certain header fields with the results of the execution
        header.state_root = db.state_root(execution_outcome.state())?;
        header.gas_used = gas_used;

        let receipts = execution_outcome.receipts_by_block(header.number);

        // update logs bloom
        let receipts_with_bloom = receipts
            .iter()
            .map(|r| r.as_ref().unwrap().bloom_slow())
            .collect::<Vec<Bloom>>();
        header.logs_bloom = receipts_with_bloom
            .iter()
            .fold(Bloom::ZERO, |bloom, r| bloom | *r);

        // update receipts root
        header.receipts_root = {
            let receipts_root = execution_outcome
                .receipts_root_slow(header.number)
                .expect("Receipts is present");

            receipts_root
        };
        trace!(target: "consensus::auto", root=?header.state_root, ?body, "calculated root");

        // finally insert into storage
        self.insert_new_block(header.clone(), body);

        // set new header with hash that should have been updated by insert_new_block
        let new_header = header.seal(self.best_hash);

        Ok((new_header, execution_outcome))
    }
}

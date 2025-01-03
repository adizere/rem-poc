use reth::api::FullNodeTypes;
use reth::builder::components::ConsensusBuilder;
use reth::builder::BuilderContext;
use reth::primitives::{Header, SealedHeader, U256};
use reth_chainspec::ChainSpec;
use reth_consensus::{Consensus, ConsensusError, PostExecutionInput};
use reth_primitives::{BlockWithSenders, SealedBlock};
use std::sync::Arc;

/// Mirrors `EthereumConsensusBuilder` except we use it to
/// instantiate MalachiteConsensus, not AutoSealConsensus
#[derive(Debug, Default, Clone, Copy)]
pub struct MalachiteConsensusBuilder {
    // TODO add closure to modify consensus
}

impl<Node> ConsensusBuilder<Node> for MalachiteConsensusBuilder
where
    Node: FullNodeTypes,
{
    type Consensus = Arc<dyn Consensus>;

    // adi: probably the execution client side
    async fn build_consensus(self, ctx: &BuilderContext<Node>) -> eyre::Result<Self::Consensus> {
        Ok(Arc::new(MalachiteConsensus::new(ctx.chain_spec())))
    }
}

/// A mirror of AutoSealConsensus from reth
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct MalachiteConsensus {
    /// Configuration
    chain_spec: Arc<ChainSpec>,
}

impl MalachiteConsensus {
    /// Create a new instance of [`MalachiteConsensus`]
    pub const fn new(chain_spec: Arc<ChainSpec>) -> Self {
        Self { chain_spec }
    }
}

impl Consensus for MalachiteConsensus {
    fn validate_header(&self, _header: &SealedHeader) -> Result<(), ConsensusError> {
        Ok(())
    }

    fn validate_header_against_parent(
        &self,
        _header: &SealedHeader,
        _parent: &SealedHeader,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }

    fn validate_header_with_total_difficulty(
        &self,
        _header: &Header,
        _total_difficulty: U256,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }

    fn validate_block_pre_execution(&self, _block: &SealedBlock) -> Result<(), ConsensusError> {
        Ok(())
    }

    fn validate_block_post_execution(
        &self,
        _block: &BlockWithSenders,
        _input: PostExecutionInput<'_>,
    ) -> Result<(), ConsensusError> {
        Ok(())
    }
}

//! rem = reth + malachite proof of concept
//!
//!
//! bootstrapping and scaffolding are partly inspired from `custom-dev-node` example from reth.

#![cfg_attr(not(test), warn(unused_crate_dependencies))]

mod chain;
mod launcher;
mod mac;

use crate::launcher::MalachiteNodeLauncher;
use crate::mac::MalachiteConsensusBuilder;
use futures_util::StreamExt;
use reth::providers::BlockReader;
use reth::{
    builder::{NodeBuilder, NodeHandle},
    providers::CanonStateSubscriptions,
    tasks::TaskManager,
};
use reth_chainspec::ChainSpec;
use reth_node_core::args::DevArgs;
use reth_node_core::{args::RpcServerArgs, node_config::NodeConfig};
use reth_node_ethereum::node::EthereumAddOns;
use reth_node_ethereum::EthereumNode;
use reth_primitives::Genesis;
use std::sync::Arc;

#[tokio::main]
async fn main() -> eyre::Result<()> {
    let tasks = TaskManager::current();

    // create node config
    let node_config = NodeConfig::test()
        .dev()
        .with_dev(DevArgs {
            dev: true,
            block_max_transactions: None,
            block_time: Some(std::time::Duration::from_secs(1)),
        })
        .with_rpc(RpcServerArgs::default().with_http())
        .with_chain(custom_chain());

    println!("created the node config");

    let NodeHandle {
        node,
        node_exit_future: _,
    } = NodeBuilder::new(node_config)
        .testing_node(tasks.executor())
        .with_types::<EthereumNode>()
        .with_components(EthereumNode::components().consensus(MalachiteConsensusBuilder::default()))
        .with_add_ons::<EthereumAddOns>()
        .launch_with_fn(|builder| {
            let launcher = MalachiteNodeLauncher::new(tasks.executor(), builder.config().datadir());
            builder.launch_with(launcher)
        })
        .await?;

    // let nh = node.provider.canonical_tip();
    let bh = BlockReader::pending_block_and_receipts(&node.provider);

    println!("launched the node {:?}", bh);

    let mut notifications = node.provider.canonical_state_stream();

    loop {
        let head = notifications.next().await.unwrap();
        println!(
            "block produced #: {:?}, tx count = {}",
            head.tip().number,
            head.tip().block.body.len()
        );
    }
}

fn custom_chain() -> Arc<ChainSpec> {
    let custom_genesis = r#"
{
    "nonce": "0x42",
    "timestamp": "0x0",
    "extraData": "0x5343",
    "gasLimit": "0xa388",
    "difficulty": "0x400000000",
    "mixHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "coinbase": "0x0000000000000000000000000000000000000000",
    "alloc": {
        "0x6Be02d1d3665660d22FF9624b7BE0551ee1Ac91b": {
            "balance": "0x4a47e3c12448f4ad000000"
        }
    },
    "number": "0x0",
    "gasUsed": "0x0",
    "parentHash": "0x0000000000000000000000000000000000000000000000000000000000000000",
    "config": {
        "ethash": {},
        "chainId": 2600,
        "homesteadBlock": 0,
        "eip150Block": 0,
        "eip155Block": 0,
        "eip158Block": 0,
        "byzantiumBlock": 0,
        "constantinopleBlock": 0,
        "petersburgBlock": 0,
        "istanbulBlock": 0,
        "berlinBlock": 0,
        "londonBlock": 0,
        "terminalTotalDifficulty": 0,
        "terminalTotalDifficultyPassed": true,
        "shanghaiTime": 0
    }
}
"#;
    let genesis: Genesis = serde_json::from_str(custom_genesis).unwrap();
    Arc::new(genesis.into())
}

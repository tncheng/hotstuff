use crate::config::{Committee, Parameters};
use crate::core::Core;
use crate::error::MempoolResult;
use crate::front::Front;
use crate::synchronizer::Synchronizer;
use consensus::{ConsensusMempoolMessage, ConsensusMessage};
use crypto::{PublicKey, SignatureService};
use log::info;
use network::{NetReceiver, NetSender};
use store::Store;
use tokio::sync::mpsc::{channel, Receiver, Sender};

#[cfg(test)]
#[path = "tests/mempool_tests.rs"]
pub mod mempool_tests;

pub struct Mempool;

impl Mempool {
    pub fn run(
        name: PublicKey,
        committee: Committee,
        parameters: Parameters,
        store: Store,
        signature_service: SignatureService,
        consensus_channel: Sender<ConsensusMessage>,
        consensus_mempool_channel: Receiver<ConsensusMempoolMessage>,
    ) -> MempoolResult<()> {
        info!(
            "Mempool queue capacity set to {} payloads",
            parameters.queue_capacity
        );
        info!(
            "Mempool max payload size set to {} B",
            parameters.max_payload_size
        );
        info!(
            "Mempool min block delay set to {} ms",
            parameters.min_block_delay
        );

        let (tx_network, rx_network) = channel(1000);
        let (tx_core, rx_core) = channel(1000);
        let (tx_client, rx_client) = channel(1000);

        // Run the front end that receives client transactions.
        let address = committee.front_address(&name).map(|mut x| {
            x.set_ip("0.0.0.0".parse().unwrap());
            x
        })?;

        let front = Front::new(address, tx_client);
        tokio::spawn(async move {
            front.run().await;
        });

        // Run the mempool network sender and receiver.
        let address = committee.mempool_address(&name).map(|mut x| {
            x.set_ip("0.0.0.0".parse().unwrap());
            x
        })?;
        let network_receiver = NetReceiver::new(address, tx_core);
        tokio::spawn(async move {
            network_receiver.run().await;
        });

        let mut network_sender = NetSender::new(rx_network);
        tokio::spawn(async move {
            network_sender.run().await;
        });

        // Make the synchronizer.
        let synchronizer = Synchronizer::new(
            consensus_channel,
            store.clone(),
            name,
            committee.clone(),
            tx_network.clone(),
            parameters.sync_retry_delay,
        );

        // Run the core.
        let mut core = Core::new(
            name,
            committee,
            parameters,
            store,
            signature_service,
            synchronizer,
            /* core_channel */ rx_core,
            consensus_mempool_channel,
            /* client_channel */ rx_client,
            /* network_channel */ tx_network,
        );
        tokio::spawn(async move {
            core.run().await;
        });

        Ok(())
    }
}

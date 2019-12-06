// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Periodic rebroadcast of neighbor packets.

use std::time::{Instant, Duration};

use codec::Encode;
use futures::prelude::*;
use futures::sync::mpsc;
use log::{debug, warn};
use tokio_timer::Delay;

use network::PeerId;
use sp_runtime::traits::{NumberFor, Block as BlockT};
use super::{gossip::{NeighborPacket, GossipMessage}, Network};

// how often to rebroadcast, if no other
const REBROADCAST_AFTER: Duration = Duration::from_secs(2 * 60);

fn rebroadcast_instant() -> Instant {
	Instant::now() + REBROADCAST_AFTER
}

/// A sender used to send neighbor packets to a background job.
#[derive(Clone)]
pub(super) struct NeighborPacketSender<B: BlockT>(
	mpsc::UnboundedSender<(Vec<PeerId>, NeighborPacket<NumberFor<B>>)>
);

impl<B: BlockT> NeighborPacketSender<B> {
	/// Send a neighbor packet for the background worker to gossip to peers.
	pub fn send(
		&self,
		who: Vec<network::PeerId>,
		neighbor_packet: NeighborPacket<NumberFor<B>>,
	) {
		if let Err(err) = self.0.unbounded_send((who, neighbor_packet)) {
			debug!(target: "afg", "Failed to send neighbor packet: {:?}", err);
		}
	}
}

/// Does the work of sending neighbor packets, asynchronously.
///
/// It may rebroadcast the last neighbor packet periodically when no
/// progress is made.
pub(super) fn neighbor_packet_worker<B, N>(net: N) -> (
	impl Future<Item = (), Error = ()> + Send + 'static,
	NeighborPacketSender<B>,
) where
	B: BlockT,
	N: Network<B>,
{
	let mut last = None;
	let (tx, mut rx) = mpsc::unbounded::<(Vec<PeerId>, NeighborPacket<NumberFor<B>>)>();
	let mut delay = Delay::new(rebroadcast_instant());

	let work = futures::future::poll_fn(move || {
		loop {
			match rx.poll().expect("unbounded receivers do not error; qed") {
				Async::Ready(None) => return Ok(Async::Ready(())),
				Async::Ready(Some((to, packet))) => {
					// send to peers.
					net.send_message(to.clone(), GossipMessage::<B>::from(packet.clone()).encode());

					// rebroadcasting network.
					delay.reset(rebroadcast_instant());
					last = Some((to, packet));
				}
				Async::NotReady => break,
			}
		}

		// has to be done in a loop because it needs to be polled after
		// re-scheduling.
		loop {
			match delay.poll() {
				Err(e) => {
					warn!(target: "afg", "Could not rebroadcast neighbor packets: {:?}", e);
					delay.reset(rebroadcast_instant());
				}
				Ok(Async::Ready(())) => {
					delay.reset(rebroadcast_instant());

					if let Some((ref to, ref packet)) = last {
						// send to peers.
						net.send_message(to.clone(), GossipMessage::<B>::from(packet.clone()).encode());
					}
				}
				Ok(Async::NotReady) => return Ok(Async::NotReady),
			}
		}
	});

	(work, NeighborPacketSender(tx))
}
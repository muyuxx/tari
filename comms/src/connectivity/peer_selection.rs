//  Copyright 2020, The Tari Project
//
//  Redistribution and use in source and binary forms, with or without modification, are permitted provided that the
//  following conditions are met:
//
//  1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following
//  disclaimer.
//
//  2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the
//  following disclaimer in the documentation and/or other materials provided with the distribution.
//
//  3. Neither the name of the copyright holder nor the names of its contributors may be used to endorse or promote
//  products derived from this software without specific prior written permission.
//
//  THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND ANY EXPRESS OR IMPLIED WARRANTIES,
//  INCLUDING, BUT NOT LIMITED TO, THE IMPLIED WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
//  DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE FOR ANY DIRECT, INDIRECT, INCIDENTAL,
//  SPECIAL, EXEMPLARY, OR CONSEQUENTIAL DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
//  SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER CAUSED AND ON ANY THEORY OF LIABILITY,
//  WHETHER IN CONTRACT, STRICT LIABILITY, OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE
//  USE OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use super::error::ConnectivityError;
use crate::{
    peer_manager::{NodeId, Peer, PeerQuery, PeerQuerySortBy},
    PeerManager,
};

const LOG_TARGET: &str = "comms::connectivity::peer_selection";

pub async fn select_neighbours(
    peer_manager: &PeerManager,
    node_id: &NodeId,
    n: usize,
) -> Result<Vec<Peer>, ConnectivityError>
{
    // Fetch to all n nearest neighbour Communication Nodes
    // which are eligible for connection.
    // Currently that means:
    // - The peer isn't banned,
    // - it has the required features
    // - it didn't recently fail to connect, and
    // - it is not in the exclusion list in closest_request
    let mut connect_ineligible_count = 0;
    let mut banned_count = 0;
    let mut filtered_out_node_count = 0;
    let query = PeerQuery::new()
        .select_where(|peer| {
            if peer.is_banned() {
                trace!(target: LOG_TARGET, "[{}] is banned", peer.node_id);
                banned_count += 1;
                return false;
            }

            if !peer.features.contains(features) {
                trace!(
                    target: LOG_TARGET,
                    "[{}] is does not have the required features {:?}",
                    peer.node_id,
                    features
                );
                filtered_out_node_count += 1;
                return false;
            }

            let is_connect_eligible = {
                !peer.is_offline() &&
                    // Check this peer was recently connectable
                    (peer.connection_stats.failed_attempts() <= config.broadcast_cooldown_max_attempts ||
                        peer.connection_stats
                            .time_since_last_failure()
                            .map(|failed_since| failed_since >= config.broadcast_cooldown_period)
                            .unwrap_or(true))
            };

            if !is_connect_eligible {
                trace!(
                    target: LOG_TARGET,
                    "[{}] suffered too many connection attempt failures or is offline",
                    peer.node_id
                );
                connect_ineligible_count += 1;
                return false;
            }

            true
        })
        .sort_by(PeerQuerySortBy::DistanceFrom(&node_id))
        .limit(n);

    let peers = peer_manager.perform_query(query).await?;
    let total_excluded = banned_count + connect_ineligible_count + filtered_out_node_count;
    if total_excluded > 0 {
        debug!(
            target: LOG_TARGET,
            "\n====================================\n Closest Peer Selection\n\n {num_peers} peer(s) selected\n \
             {total} peer(s) were not selected \n\n {banned} banned\n {filtered_out} not communication node\n \
             {not_connectable} are not connectable\n 
             \n====================================\n",
            num_peers = peers.len(),
            total = total_excluded,
            banned = banned_count,
            filtered_out = filtered_out_node_count,
            not_connectable = connect_ineligible_count,
        );
    }

    Ok(peers)
}

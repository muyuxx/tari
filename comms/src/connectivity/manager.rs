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

use crate::{
    connection_manager::ConnectionManagerRequester,
    connectivity::{
        config::ConnectivityConfig,
        error::ConnectivityError,
        peer_pool::{PeerPool, PeerPoolType, PoolId, PoolParams},
        peer_pools::PeerPools,
        peer_selection,
        requester::ConnectivityRequest,
    },
    peer_manager::{NodeId, Peer},
    ConnectionManagerEvent,
    NodeIdentity,
    PeerConnection,
    PeerManager,
};
use futures::{channel::mpsc, stream::Fuse, StreamExt};
use log::*;
use std::sync::Arc;
use tari_shutdown::ShutdownSignal;
use tokio::{task, task::JoinHandle};

const LOG_TARGET: &str = "comms::connectivity::manager";

pub struct ConnectivityManager {
    pub config: ConnectivityConfig,
    pub request_rx: mpsc::Receiver<ConnectivityRequest>,
    pub node_identity: Arc<NodeIdentity>,
    pub connection_manager: ConnectionManagerRequester,
    pub peer_manager: Arc<PeerManager>,
    pub shutdown_signal: ShutdownSignal,
}

impl ConnectivityManager {
    pub fn create(self) -> ConnectivityManagerActor {
        ConnectivityManagerActor {
            config: self.config,
            request_rx: self.request_rx.fuse(),
            active_pools: PeerPools::new(),
            ad_hoc_pool: Vec::new(),
            node_identity: self.node_identity,
            connection_manager: self.connection_manager,
            peer_manager: self.peer_manager,
            shutdown_signal: Some(self.shutdown_signal),
        }
    }

    pub fn spawn(self) -> JoinHandle<()> {
        self.create().spawn()
    }
}

struct ConnectivityManagerActor {
    config: ConnectivityConfig,
    active_pools: PeerPools,
    ad_hoc_pool: Vec<PeerConnection>,
    client_node_pool: Vec<PeerConnection>,
    request_rx: Fuse<mpsc::Receiver<ConnectivityRequest>>,
    node_identity: Arc<NodeIdentity>,
    connection_manager: ConnectionManagerRequester,
    peer_manager: Arc<PeerManager>,
    shutdown_signal: Option<ShutdownSignal>,
}

impl ConnectivityManagerActor {
    pub fn spawn(self) -> JoinHandle<()> {
        task::spawn(Self::run(self))
    }

    pub async fn run(mut self) {
        let mut shutdown_signal = self
            .shutdown_signal
            .take()
            .expect("ConnectivityManager initialized without a shutdown_signal");

        let mut connection_manager_events = self.connection_manager.get_event_subscription();

        loop {
            futures::select! {
                req = self.request_rx.select_next_some() => {
                    self.handle_request(req).await;
                },

                event = connection_manager_event.select_next_some() => {
                    if let Ok(event) = event {
                        self.handle_connection_manager_event(&event).await;
                    }
                },

                _ = shutdown_signal => {
                    info!(target: LOG_TARGET, "ConnectivityManager is shutting down because it received the shutdown signal");
                    break;
                }
            }
        }
    }

    async fn handle_request(&mut self, req: ConnectivityRequest) {
        use ConnectivityRequest::*;
        match req {
            AddPool(pool_type, reply_tx) => {
                if self.active_pools.iter().any(|pool| pool.pool_type() == pool_type) {
                    let _ = reply_tx.send(Ok(()));
                    return;
                }

                let _ = reply_tx.send(self.add_pool(pool_type).await);
            },
            ReleasePool(pool_type) => {},
            GetPool(pool_type, reply_tx) => {},
            SelectConnections(selection, reply_tx) => {},
            BanPeer(node_id) => {
                if let Err(err) = self.ban_peer(&node_id).await {
                    error!(target: LOG_TARGET, "Error when banning peer: {:?}", err);
                }
            },
        }
    }

    async fn handle_connection_manager_event(&mut self, event: &ConnectionManagerEvent) {
        use ConnectionManagerEvent::*;
        match event {
            PeerConnected(conn) => {
                // TODO::
            },
            PeerDisconnected(node_id) => {
                // TODO:
            },
            PeerConnectWillClose(_, _, _) => {
                // TODO:
            },
            _ => {},
        }
    }

    async fn add_pool(&mut self, pool_type: PeerPoolType) -> Result<(), ConnectivityError> {
        let pool = PeerPool::new(pool_type, self.get_pool_params_by_type(pool_type));
        let pool_id = pool.id();
        self.active_pools.push(pool);
        self.refresh_pool_if_stale(pool_id).await?;
        Ok(())
    }

    fn get_pool_params_by_type(&self, pool_type: PeerPoolType) -> PoolParams {
        use PeerPoolType::*;
        match pool_type {
            Neighbours => PoolParams {
                num_desired: self.config.desired_neighbouring_pool_size,
                stale_interval: self.config.neighbouring_pool_refresh_interval,
                min_required: None,
            },
            Random => PoolParams {
                num_desired: self.config.desired_random_pool_size,
                stale_interval: self.config.random_pool_refresh_interval,
                min_required: Some(0),
            },
        }
    }

    async fn refresh_pool_if_stale(&mut self, pool_id: PoolId) -> Result<(), ConnectivityError> {
        let pool = self
            .active_pools
            .get_mut(pool_id)
            .ok_or_else(|| ConnectivityError::PoolNotFoundbyId)?;

        if !pool.is_stale() {
            debug!(target: LOG_TARGET, "Peer pool is still fresh: {}", pool);
            return Ok(());
        }

        let (new_peers, stale_peers) = self.get_changes(&pool).await?;

        match self.pool_type {
            PeerPoolType::Neighbours => task::spawn(Self::refresh_neighbour_pool(
                config,
                peer_manager,
                connection_manager,
                pool_id,
            )),
            PeerPoolType::Random => task::spawn(Self::refresh_random_pool(
                config,
                peer_manager,
                connection_manager,
                pool_id,
            )),
        }
    }

    async fn get_changes(&self, pool: &PeerPool) -> Result<(Vec<NodeId>, Vec<NodeId>), ConnectivityError> {
        let mut new_peers = self.select_peers(pool).await?;
        let existing_connections = pool.connections();

        let (keep, to_disconnect) = existing_connections
            .into_iter()
            .partition::<Vec<_>, _>(|conn| new_peers.contains(conn.peer_node_id()));

        Ok((vec![], vec![]))
    }

    async fn select_peers(&self, pool: &PeerPool) -> Result<Vec<NodeId>, ConnectivityError> {
        use PeerPoolType::*;
        let peers = match pool.pool_type() {
            Neighbours => {
                peer_selection::select_neighbours(
                    &self.peer_manager,
                    self.node_identity.node_id(),
                    pool.params().num_desired,
                )
                .await?
            },
            Random => {
                let neighbours = self
                    .active_pools
                    .get_by_type(PeerPoolType::Neighbours)
                    .map(|pool| pool.get_node_ids())
                    .unwrap_or_else(Vec::new);

                self.peer_manager
                    .random_peers(pool.params().num_desired, &excluded)
                    .await?
            },
        };

        Ok(peer.into_iter().map(|p| p.node_id).collect())
    }

    async fn refresh_neighbour_pool(
        config: ConnectivityConfig,
        peer_manager: Arc<PeerManager>,
        connection_manager: ConnectionManagerRequester,
        pool_id: PoolId,
    ) -> Result<(), ConnectivityError>
    {
    }

    async fn refresh_random_pool(
        config: ConnectivityConfig,
        peer_manager: Arc<PeerManager>,
        connection_manager: ConnectionManagerRequester,
        pool_id: PoolId,
    ) -> Result<(), ConnectivityError>
    {
    }

    async fn ban_peer(&self, node_id: &NodeId) -> Result<(), ConnectivityError> {
        Ok(())
    }
}

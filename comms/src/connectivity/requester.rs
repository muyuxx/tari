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

use super::{
    error::ConnectivityError,
    peer_pool::{PeerPool, PeerPoolType},
};
use crate::{peer_manager::NodeId, PeerConnection};
use futures::{
    channel::{mpsc, oneshot},
    SinkExt,
};

#[derive(Debug)]
pub enum ConnectivityRequest {
    AddPool(PeerPoolType, oneshot::Sender<Result<(), ConnectivityError>>),
    ReleasePool(PeerPoolType),
    GetPool(PeerPoolType, oneshot::Sender<Result<PeerPool, ConnectivityError>>),
    SelectConnections(
        ConnectivitySelection,
        oneshot::Sender<Result<Vec<PeerConnection>, ConnectivityError>>,
    ),
    BanPeer(Box<NodeId>),
}

#[derive(Debug, Clone)]
pub enum ConnectivitySelection {
    Propagation { num_neighbour: usize, num_random: usize },
    Single(Box<NodeId>),
}

#[derive(Clone)]
pub struct ConnectivityRequester {
    sender: mpsc::Sender<ConnectivityRequest>,
}

impl ConnectivityRequester {
    pub fn new(sender: mpsc::Sender<ConnectivityRequest>) -> Self {
        Self { sender }
    }

    pub async fn add_pool(&mut self, pool_type: PeerPoolType) -> Result<(), ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::AddPool(pool_type, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)?
    }

    pub async fn release_pool(&mut self, pool_type: PeerPoolType) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::ReleasePool(pool_type))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }

    pub async fn get_pool(&mut self, pool_type: PeerPoolType) -> Result<PeerPool, ConnectivityError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::GetPool(pool_type, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)?
    }

    pub async fn select_connections(
        &mut self,
        selection: ConnectivitySelection,
    ) -> Result<Vec<PeerConnection>, ConnectivityError>
    {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.sender
            .send(ConnectivityRequest::SelectConnections(selection, reply_tx))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        reply_rx.await.map_err(|_| ConnectivityError::ActorResponseCancelled)?
    }

    pub async fn ban_peer(&mut self, node_id: NodeId) -> Result<(), ConnectivityError> {
        self.sender
            .send(ConnectivityRequest::BanPeer(Box::new(node_id)))
            .await
            .map_err(|_| ConnectivityError::ActorDisconnected)?;
        Ok(())
    }
}

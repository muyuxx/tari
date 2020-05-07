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
    connectivity::{config::ConnectivityConfig, error::ConnectivityError, manager::ConnectivityManager},
    peer_manager::NodeId,
    PeerConnection,
    PeerManager,
};
use futures::channel::oneshot;
use std::{
    fmt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::{task, task::JoinHandle};

pub type PoolId = usize;

#[derive(Clone, Debug, Copy, PartialEq, Eq)]
pub enum PeerPoolType {
    Neighbours,
    Random,
}

fn get_next_id() -> PoolId {
    static ID_COUNTER: AtomicUsize = AtomicUsize::new(0);
    ID_COUNTER.fetch_add(1, Ordering::SeqCst)
}

#[derive(Clone)]
pub struct PeerPoolHandle(Arc<PoolId>);

#[derive(Debug)]
pub struct PeerPool {
    id: PoolId,
    status: PoolStatus,
    params: PoolParams,
    connections: Vec<PeerConnection>,
    pool_type: PeerPoolType,
    last_refreshed: Option<Instant>,
    refresh_in_progress: bool,
}

enum PoolStatus {
    Uninitialized,
    Ok,
    Partial,
    Failed,
}

pub struct PoolParams {
    pub num_desired: usize,
    pub stale_interval: Duration,
    pub min_required: Option<usize>,
}

impl PeerPool {
    pub fn new(pool_type: PeerPoolType, params: PoolParams) -> Self {
        Self {
            id: get_next_id(),
            pool_type,
            params,
            status: PoolStatus::Uninitialized,
            connections: Vec::new(),
            last_refreshed: None,
            refresh_in_progress: false,
        }
    }

    pub fn id(&self) -> PoolId {
        self.id
    }

    pub fn pool_type(&self) -> &PoolType {
        &self.pool_type
    }

    pub fn params(&self) -> &PoolParams {
        &self.params
    }

    pub fn is_stale(&self) -> bool {
        self.last_refreshed
            .map(|instant| instant.elapsed() > self.params.stale_interval)
            .unwrap_or(true)
    }

    pub fn get_node_ids(&self) -> Vec<NodeId> {
        self.connections
            .iter()
            .map(|conn| conn.peer_node_id())
            .cloned()
            .collect()
    }

    pub fn connections(&self) -> &[PeerConnection] {
        &self.connections
    }
}

impl fmt::Display for PeerPool {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Id: {}, Type: {:?}, {} active connections, Last Refreshed: {}",
            self.id,
            self.pool_type,
            self.connections,
            self.last_refreshed
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_else(|| "Never".to_string())
        )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use std::{iter::repeat_with, thread};

    #[test]
    fn get_next_id_increment_thread_safety() {
        let join_handle1 = thread::spawn(|| repeat_with(get_next_id).take(10).collect::<Vec<_>>());
        let join_handle2 = thread::spawn(|| repeat_with(get_next_id).take(10).collect::<Vec<_>>());

        let ids1 = join_handle1.join().unwrap();
        let ids2 = join_handle2.join().unwrap();

        assert!(ids2.iter().all(|id| !ids1.contains(id)));
    }
}

use crate::{Node, NodeId, TypeConfig};
use openraft::error::{InstallSnapshotError, RPCError, RaftError, RemoteError, Unreachable};
use openraft::RaftNetworkFactory;

/// Network implementation for Raft
pub struct Network;

impl RaftNetworkFactory<TypeConfig> for Network {
    type Network = NetworkConnection;

    async fn new_client(&mut self, target: NodeId, node: &Node) -> Self::Network {
        NetworkConnection {
            target,
            target_node: node.clone(),
        }
    }
}

/// Connection instance for a specific target node.
pub struct NetworkConnection {
    target: NodeId,
    target_node: Node,
}

impl NetworkConnection {
    fn make_url(&self, path: &str) -> String {
        format!("http://{}{}", self.target_node.addr, path)
    }

    fn map_target_err<E: std::error::Error>(&self, e: E) -> RPCError<NodeId, Node, E> {
        RPCError::RemoteError(RemoteError::new(self.target, e))
    }
}

impl openraft::RaftNetwork<TypeConfig> for NetworkConnection {
    async fn append_entries(
        &mut self,
        rpc: openraft::raft::AppendEntriesRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::AppendEntriesResponse<NodeId>,
        RPCError<NodeId, Node, RaftError<NodeId>>,
    > {
        let client = reqwest::Client::new();
        let body =
            serde_json::to_vec(&rpc).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let resp = client
            .post(self.make_url("/raft/append_entries"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let result: Result<openraft::raft::AppendEntriesResponse<NodeId>, RaftError<NodeId>> =
            serde_json::from_slice(&bytes)
                .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        result.map_err(|e| self.map_target_err(e))
    }

    async fn install_snapshot(
        &mut self,
        rpc: openraft::raft::InstallSnapshotRequest<TypeConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        openraft::raft::InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, Node, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let client = reqwest::Client::new();
        let body =
            serde_json::to_vec(&rpc).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let resp = client
            .post(self.make_url("/raft/install_snapshot"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let result: Result<
            openraft::raft::InstallSnapshotResponse<NodeId>,
            RaftError<NodeId, InstallSnapshotError>,
        > = serde_json::from_slice(&bytes)
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        result.map_err(|e| self.map_target_err(e))
    }

    async fn vote(
        &mut self,
        rpc: openraft::raft::VoteRequest<NodeId>,
        _option: openraft::network::RPCOption,
    ) -> Result<openraft::raft::VoteResponse<NodeId>, RPCError<NodeId, Node, RaftError<NodeId>>>
    {
        let client = reqwest::Client::new();
        let body =
            serde_json::to_vec(&rpc).map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let resp = client
            .post(self.make_url("/raft/vote"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        let result: Result<openraft::raft::VoteResponse<NodeId>, RaftError<NodeId>> =
            serde_json::from_slice(&bytes)
                .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;

        result.map_err(|e| self.map_target_err(e))
    }
}

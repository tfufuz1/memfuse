#![forbid(unsafe_code)]
#![allow(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]

//! # MemFuse Cluster — Distributed Consensus & Replication
//!
//! This crate implements the Raft-based replication layer for MemFuse,
//! enabling high availability and distributed consistency across nodes.
//! It utilizes `openraft` as the underlying consensus engine.

/// Network module for Raft communication
pub mod network;
#[allow(missing_docs)]
pub mod node;
/// Storage module for Raft entries and snapshots
#[allow(missing_docs)]
pub mod storage;

use serde::{Deserialize, Serialize};
use std::io::Cursor;

/// Cluster-wide Node ID
pub type NodeId = u64;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Hash, Default)]
/// Node Information
pub struct Node {
    /// Network address of the node
    pub addr: String,
}

openraft::declare_raft_types!(
    pub TypeConfig:
        D = Vec<u8>,
        R = Vec<u8>,
        Node = Node,
);

impl serde::Serialize for TypeConfig {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_unit()
    }
}

impl<'de> serde::Deserialize<'de> for TypeConfig {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct Visitor;
        impl<'de> serde::de::Visitor<'de> for Visitor {
            type Value = TypeConfig;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("struct TypeConfig")
            }
            fn visit_unit<E>(self) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                Ok(TypeConfig {})
            }
        }
        deserializer.deserialize_unit(Visitor)
    }
}

impl std::fmt::Display for TypeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TypeConfig")
    }
}

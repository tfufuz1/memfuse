//! Core type definitions for Project Chimera.
//!
//! This module contains all fundamental data types used throughout
//! the Tri-Hybrid RAG Database system.

use crate::error::{ChimeraError, Result};
use bytecheck::CheckBytes;
use rkyv::{Archive, Deserialize as RkyvDeserialize, Serialize as RkyvSerialize};
use serde::{Deserialize, Serialize};

/// Unique identifier for documents in the database.
///
/// Documents are the primary unit of storage, containing an embedding,
/// metadata payload, and optional graph entities.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct DocId(pub u64);

impl DocId {
    /// Maximum possible document ID.
    pub const MAX: Self = Self(u64::MAX);
    /// Minimum possible document ID.
    pub const MIN: Self = Self(0);

    /// Creates a new document ID from a u64 value.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl From<u64> for DocId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for DocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "DocId({})", self.0)
    }
}

/// Unique identifier for entities (nodes) in the graph index.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct EntityId(pub u64);

impl EntityId {
    /// Maximum possible entity ID.
    pub const MAX: Self = Self(u64::MAX);
    /// Minimum possible entity ID.
    pub const MIN: Self = Self(0);

    /// Creates a new entity ID from a u64 value.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl From<u64> for EntityId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for EntityId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EntityId({})", self.0)
    }
}

/// Internal identifier for nodes within graph structures (e.g. CSR).
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct NodeId(pub u32);

impl NodeId {
    /// Maximum possible node ID.
    pub const MAX: Self = Self(u32::MAX);
    /// Minimum possible node ID.
    pub const MIN: Self = Self(0);

    /// Creates a new node ID from a u32 value.
    #[inline]
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    /// Returns the inner u32 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0 as u64
    }
}

impl From<u32> for NodeId {
    fn from(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for NodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "NodeId({})", self.0)
    }
}

/// 3D point in space for spatial indexing.
///
/// Used by the Spatial Index (Octree) to represent geometric locations
/// of documents in 3D space, enabling efficient range queries.
#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug, PartialEq))]
pub struct Point3D {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

impl Point3D {
    /// Creates a new 3D point.
    #[inline]
    pub const fn new(x: f32, y: f32, z: f32) -> Self {
        Self { x, y, z }
    }

    /// Returns the origin point (0, 0, 0).
    #[inline]
    pub const fn origin() -> Self {
        Self::new(0.0, 0.0, 0.0)
    }

    /// Computes the Euclidean distance to another point.
    pub fn distance_to(&self, other: &Point3D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        (dx * dx + dy * dy + dz * dz).sqrt()
    }

    /// Computes the squared distance to another point (faster, no sqrt).
    #[inline]
    pub fn distance_squared_to(&self, other: &Point3D) -> f32 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        let dz = self.z - other.z;
        dx * dx + dy * dy + dz * dz
    }
}

/// Axis-aligned bounding box in 3D space.
///
/// Represents a rectangular volume defined by minimum and maximum corners.
/// Used for spatial queries and octree node bounds.
#[derive(
    Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive(compare(PartialEq))]
#[archive_attr(derive(CheckBytes, Debug, PartialEq))]
pub struct BoundingBox {
    pub min: Point3D,
    pub max: Point3D,
}

impl BoundingBox {
    /// Creates a new bounding box. Validates that min coordinates are <= max.
    pub fn try_new(min: Point3D, max: Point3D) -> Result<Self> {
        if min.x > max.x || min.y > max.y || min.z > max.z {
            return Err(ChimeraError::invalid_input(format!(
                "Invalid bounding box: min ({:?}) must be <= max ({:?})",
                min, max
            )));
        }
        Ok(Self { min, max })
    }

    /// Legacy constructor that falls back to a minimal box on invalid input.
    /// Use try_new where possible (Zero-Panic Policy).
    pub fn new(min: Point3D, max: Point3D) -> Self {
        Self::try_new(min, max).unwrap_or(Self { min, max: min })
    }

    /// Returns true if this bounding box contains the given point.
    #[inline]
    pub fn contains(&self, point: &Point3D) -> bool {
        point.x >= self.min.x
            && point.x <= self.max.x
            && point.y >= self.min.y
            && point.y <= self.max.y
            && point.z >= self.min.z
            && point.z <= self.max.z
    }

    /// Returns true if this bounding box intersects with another.
    pub fn intersects(&self, other: &BoundingBox) -> bool {
        self.min.x <= other.max.x
            && self.max.x >= other.min.x
            && self.min.y <= other.max.y
            && self.max.y >= other.min.y
            && self.min.z <= other.max.z
            && self.max.z >= other.min.z
    }

    /// Returns true if this bounding box intersects with a sphere.
    pub fn intersects_sphere(&self, center: &Point3D, radius: f32) -> bool {
        let radius_sq = radius * radius;
        let mut dist_sq = 0.0;

        // For each axis, add squared distance if point is outside box
        for i in 0..3 {
            let (c, min, max) = match i {
                0 => (center.x, self.min.x, self.max.x),
                1 => (center.y, self.min.y, self.max.y),
                _ => (center.z, self.min.z, self.max.z),
            };

            if c < min {
                dist_sq += (min - c) * (min - c);
            } else if c > max {
                dist_sq += (c - max) * (c - max);
            }
        }

        dist_sq <= radius_sq
    }

    /// Returns the center point of this bounding box.
    pub fn center(&self) -> Point3D {
        Point3D::new(
            (self.min.x + self.max.x) / 2.0,
            (self.min.y + self.max.y) / 2.0,
            (self.min.z + self.max.z) / 2.0,
        )
    }

    /// Returns the size (width, height, depth) of this bounding box.
    pub fn size(&self) -> Point3D {
        Point3D::new(
            self.max.x - self.min.x,
            self.max.y - self.min.y,
            self.max.z - self.min.z,
        )
    }
}

/// Unique identifier for edges (relations) in the graph index.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct EdgeId(pub u64);

impl EdgeId {
    /// Creates a new edge ID from a u64 value.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl From<u64> for EdgeId {
    fn from(id: u64) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for EdgeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "EdgeId({})", self.0)
    }
}

/// Transaction identifier for coordinating multi-index operations.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct TxId(pub u64);

impl TxId {
    /// Creates a new transaction ID from a u64 value.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for TxId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "TxId({})", self.0)
    }
}

/// Unique identifier for an Agent or User interacting with the system.
#[derive(
    Debug,
    Clone,
    Copy,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
#[repr(transparent)]
pub struct AgentId(pub u64);

impl AgentId {
    /// Creates a new agent ID.
    #[inline]
    pub const fn new(id: u64) -> Self {
        Self(id)
    }

    /// Returns the inner u64 value.
    #[inline]
    pub const fn inner(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for AgentId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "AgentId({})", self.0)
    }
}

/// Unique identifier for a Namespace to provide multi-tenant isolation.
///
/// Namespaces group collections and provide the highest level of data isolation.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct NamespaceId(pub std::sync::Arc<str>);

impl rkyv::Archive for NamespaceId {
    type Archived = rkyv::string::ArchivedString;
    type Resolver = rkyv::string::StringResolver;

    unsafe fn resolve(&self, pos: usize, resolver: Self::Resolver, out: *mut Self::Archived) {
        rkyv::string::ArchivedString::resolve_from_str(self.0.as_ref(), pos, resolver, out);
    }
}

impl<S: rkyv::ser::Serializer + ?Sized> rkyv::Serialize<S> for NamespaceId {
    fn serialize(&self, serializer: &mut S) -> std::result::Result<Self::Resolver, S::Error> {
        rkyv::string::ArchivedString::serialize_from_str(self.0.as_ref(), serializer)
    }
}

impl<D: rkyv::Fallible + ?Sized> rkyv::Deserialize<NamespaceId, D>
    for rkyv::string::ArchivedString
{
    fn deserialize(&self, _deserializer: &mut D) -> std::result::Result<NamespaceId, D::Error> {
        Ok(NamespaceId(std::sync::Arc::from(self.as_str())))
    }
}

impl NamespaceId {
    /// Creates a new namespace ID from a string.
    pub fn new(id: impl Into<std::sync::Arc<str>>) -> Self {
        Self(id.into())
    }

    /// Returns the default namespace "default".
    pub fn default_ns() -> Self {
        Self(std::sync::Arc::from("default"))
    }

    /// Returns the inner string slice.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Generates a key prefix for this namespace using a length-prefixed binary format.
    ///
    /// # SPEC-030: Absolute Namespace Isolation (Physical Index Keys)
    /// This ensures that namespaces cannot shadow each other (e.g. "prod" vs "pro").
    /// Format: [ns_len (u32, LE)][ns_bytes]
    pub fn key_prefix_bytes(&self) -> Vec<u8> {
        let ns_bytes = self.0.as_bytes();
        let mut prefix = Vec::with_capacity(4 + ns_bytes.len());
        prefix.extend_from_slice(&(ns_bytes.len() as u32).to_le_bytes());
        prefix.extend_from_slice(ns_bytes);
        prefix
    }

    /// Generates a document-specific key within this namespace.
    pub fn doc_key_bytes(&self, doc_id: DocId) -> Vec<u8> {
        let mut key = self.key_prefix_bytes();
        key.push(b'd');
        key.extend_from_slice(&doc_id.inner().to_le_bytes());
        key
    }
}

impl Default for NamespaceId {
    fn default() -> Self {
        Self::default_ns()
    }
}

impl From<&str> for NamespaceId {
    fn from(s: &str) -> Self {
        Self(std::sync::Arc::from(s))
    }
}

impl From<String> for NamespaceId {
    fn from(s: String) -> Self {
        Self(std::sync::Arc::from(s.as_str()))
    }
}

impl std::fmt::Display for NamespaceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// A document ID qualified by its namespace.
///
/// Used for global identification of documents across the entire system.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
pub struct QualifiedDocId {
    pub namespace: NamespaceId,
    pub doc_id: DocId,
}

impl QualifiedDocId {
    /// Creates a new qualified document ID.
    pub fn new(namespace: NamespaceId, doc_id: DocId) -> Self {
        Self { namespace, doc_id }
    }

    /// Returns the namespace ID.
    pub fn namespace(&self) -> NamespaceId {
        self.namespace.clone()
    }

    /// Returns the document ID.
    pub fn doc_id(&self) -> DocId {
        self.doc_id
    }

    /// Returns the physical storage key as bytes.
    pub fn to_key_bytes(&self) -> Vec<u8> {
        self.namespace.doc_key_bytes(self.doc_id)
    }
}

impl std::fmt::Display for QualifiedDocId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.namespace, self.doc_id)
    }
}

/// SPEC-042: Memory tier for Agentic Memory routing.
///
/// Models LLM-agent memory as a three-tier system that maps directly onto
/// existing ChimeraDB subsystems — no new storage engine required:
/// - `Working`  → `TxBuffer` (uncommitted, volatile, <tx_timeout)
/// - `Episodic` → WAL + SeqNo timestamp index (per AgentId, append-only)
/// - `Semantic` → HNSW + Metadata (consolidated, persistent, indexed)
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug, PartialEq, Eq, Hash))]
pub enum MemoryTier {
    /// Volatile transaction context — lives in TxBuffer only.
    Working,
    /// Time-ordered action history per AgentId — backed by WAL SeqNo index.
    Episodic,
    /// Consolidated semantic knowledge — indexed in HNSW + Metadata.
    Semantic,
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Working => write!(f, "working"),
            Self::Episodic => write!(f, "episodic"),
            Self::Semantic => write!(f, "semantic"),
        }
    }
}

/// SPEC-041: Multimodal raw content for In-Database Embedding.
///
/// ChimeraDB accepts raw content and generates the embedding locally via
/// the `chimera-compute` crate (candle + GGUF model), eliminating external
/// API calls entirely. Only available with `--features compute`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RawContent {
    /// UTF-8 text input (nomic-embed-text, E5, BGE, etc.)
    Text(String),
    /// Raw image bytes (PNG/JPEG) — requires vision embedding model.
    Image(Vec<u8>),
    /// Raw audio bytes (WAV/FLAC) — requires audio embedding model.
    Audio(Vec<u8>),
}

impl RawContent {
    /// Returns a short type label for metrics and logging.
    pub fn type_label(&self) -> &'static str {
        match self {
            Self::Text(_) => "text",
            Self::Image(_) => "image",
            Self::Audio(_) => "audio",
        }
    }

    /// Returns the byte size of the raw content.
    pub fn byte_len(&self) -> usize {
        match self {
            Self::Text(s) => s.len(),
            Self::Image(b) | Self::Audio(b) => b.len(),
        }
    }
}

/// Vector embedding representation.
///
/// Embeddings are dense vectors typically produced by neural networks
/// for semantic similarity search.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Embedding {
    /// Dense vector data (f32 for compatibility with most ML models).
    pub data: Vec<f32>,
}

impl Embedding {
    /// Creates a new embedding from a vector of f32 values.
    pub fn new(data: Vec<f32>) -> Self {
        Self { data }
    }

    /// Returns the dimensionality of the embedding.
    #[inline]
    pub fn dim(&self) -> usize {
        self.data.len()
    }

    /// Returns the embedding data as a slice.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.data
    }

    /// Computes the L2 norm (magnitude) of the embedding.
    pub fn l2_norm(&self) -> f32 {
        self.data.iter().map(|x| x * x).sum::<f32>().sqrt()
    }

    /// Returns a normalized copy of this embedding.
    pub fn normalize(&self) -> Self {
        let norm = self.l2_norm();
        if norm == 0.0 {
            return self.clone();
        }
        let normalized: Vec<f32> = self.data.iter().map(|x| x / norm).collect();
        Self::new(normalized)
    }
}

/// Document payload containing metadata.
#[derive(
    Debug, Clone, Default, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Payload {
    /// Raw JSON or binary metadata.
    pub data: Vec<u8>,
}

impl Payload {
    /// Creates a new payload from bytes.
    pub fn new(data: Vec<u8>) -> Self {
        Self { data }
    }

    /// Creates a payload from a JSON-serializable value.
    pub fn from_json<T: Serialize>(value: &T) -> Result<Self> {
        let json = serde_json::to_vec(value)?;
        Ok(Self { data: json })
    }

    /// Deserializes the payload as JSON.
    pub fn as_json<T: for<'de> Deserialize<'de>>(&self) -> Result<T> {
        Ok(serde_json::from_slice(&self.data)?)
    }

    /// Returns the payload as a byte slice.
    #[inline]
    pub fn as_bytes(&self) -> &[u8] {
        &self.data
    }

    /// Returns true if the payload is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Returns the size of the payload in bytes.
    #[inline]
    pub fn len(&self) -> usize {
        self.data.len()
    }
}

/// Complete document representation for the Quad-Hybrid database.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Document {
    /// Unique document identifier.
    pub id: DocId,
    /// Vector embedding for semantic similarity search.
    pub embedding: Embedding,
    /// Metadata payload for structured filtering.
    pub payload: Payload,
    /// Graph entities extracted from this document.
    pub entities: Vec<Entity>,
    /// Optional 3D spatial location for spatial indexing.
    pub location: Option<Point3D>,
}

impl Document {
    /// Creates a new document with the given components.
    pub fn new(id: DocId, embedding: Embedding, payload: Payload) -> Self {
        Self {
            id,
            embedding,
            payload,
            entities: Vec::new(),
            location: None,
        }
    }

    /// Creates a document with associated graph entities.
    pub fn with_entities(mut self, entities: Vec<Entity>) -> Self {
        self.entities = entities;
        self
    }

    /// Adds a spatial location to this document.
    pub fn with_location(mut self, location: Point3D) -> Self {
        self.location = Some(location);
        self
    }
}

/// Graph entity (node) representing a semantic concept.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Entity {
    /// Unique entity identifier.
    pub id: EntityId,
    /// Human-readable entity name.
    pub name: String,
    /// Entity type/category (e.g., "Person", "Organization", "Concept").
    pub entity_type: String,
    /// Optional embedding for the entity itself.
    pub embedding: Option<Embedding>,
}

impl Entity {
    /// Creates a new entity with the given properties.
    pub fn new(id: EntityId, name: String, entity_type: String) -> Self {
        Self {
            id,
            name,
            entity_type,
            embedding: None,
        }
    }

    /// Adds an embedding to this entity.
    pub fn with_embedding(mut self, embedding: Embedding) -> Self {
        self.embedding = Some(embedding);
        self
    }
}

/// Graph edge (relation) connecting two entities.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct Edge {
    /// Unique edge identifier.
    pub id: EdgeId,
    /// Source entity ID.
    pub from: EntityId,
    /// Target entity ID.
    pub to: EntityId,
    /// Relation type (e.g., "works_at", "is_a", "relates_to").
    pub relation_type: String,
    /// Edge weight for importance scoring.
    pub weight: f32,
}

impl Edge {
    /// Creates a new edge between two entities.
    pub fn new(id: EdgeId, from: EntityId, to: EntityId, relation_type: String) -> Self {
        Self {
            id,
            from,
            to,
            relation_type,
            weight: 1.0,
        }
    }

    /// Sets the weight of this edge.
    pub fn with_weight(mut self, weight: f32) -> Self {
        self.weight = weight;
        self
    }
}

/// Search result containing a document and its relevance score.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(CheckBytes, Debug))]
pub struct ScoredDocument {
    /// Document identifier.
    pub doc_id: DocId,
    /// Relevance score (higher is more relevant).
    pub score: f32,
    /// Optional full document (may be omitted for performance).
    pub document: Option<Document>,
}

impl ScoredDocument {
    /// Creates a new scored document result.
    pub fn new(doc_id: DocId, score: f32) -> Self {
        Self {
            doc_id,
            score,
            document: None,
        }
    }

    /// Attaches the full document to this result.
    pub fn with_document(mut self, document: Document) -> Self {
        self.document = Some(document);
        self
    }
}

/// Distance metrics for vector similarity search.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
    Default,
)]
#[archive_attr(derive(CheckBytes, Debug))]
pub enum DistanceMetric {
    /// Cosine similarity (1 - cosine distance).
    #[default]
    Cosine,
    /// Euclidean (L2) distance.
    Euclidean,
    /// Dot product (inner product).
    DotProduct,
}

impl DistanceMetric {
    /// Computes the distance/similarity between two vectors.
    pub fn compute(&self, a: &[f32], b: &[f32]) -> f32 {
        debug_assert_eq!(a.len(), b.len(), "Vector dimensions must match");

        match self {
            DistanceMetric::Cosine => {
                let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
                let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
                let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
                if norm_a == 0.0 || norm_b == 0.0 {
                    0.0
                } else {
                    dot / (norm_a * norm_b)
                }
            }
            DistanceMetric::Euclidean => a
                .iter()
                .zip(b.iter())
                .map(|(x, y)| (x - y).powi(2))
                .sum::<f32>()
                .sqrt(),
            DistanceMetric::DotProduct => a.iter().zip(b.iter()).map(|(x, y)| x * y).sum(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Unit tests ──────────────────────────────────────────────────────────

    #[test]
    fn doc_id_creation_and_display() {
        let id = DocId::new(42);
        assert_eq!(id.inner(), 42);
        assert_eq!(format!("{}", id), "DocId(42)");
    }

    #[test]
    fn doc_id_from_u64() {
        let id: DocId = 7u64.into();
        assert_eq!(id.inner(), 7);
    }

    #[test]
    fn entity_id_and_edge_id_display() {
        let eid = EntityId::new(100);
        assert_eq!(format!("{}", eid), "EntityId(100)");
    }

    #[test]
    fn embedding_dim_and_slice() {
        let emb = Embedding::new(vec![3.0, 4.0]);
        assert_eq!(emb.dim(), 2);
        assert_eq!(emb.as_slice(), &[3.0, 4.0]);
    }

    #[test]
    fn embedding_l2_norm() {
        let emb = Embedding::new(vec![3.0, 4.0]);
        assert!((emb.l2_norm() - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn embedding_normalize_unit_vector() {
        let emb = Embedding::new(vec![3.0, 4.0]);
        let n = emb.normalize();
        assert!((n.l2_norm() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn embedding_normalize_zero_vector_is_unchanged() {
        let zero = Embedding::new(vec![0.0, 0.0, 0.0]);
        let n = zero.normalize();
        assert_eq!(n.as_slice(), &[0.0, 0.0, 0.0]);
    }

    #[test]
    fn payload_json_roundtrip() {
        #[derive(Serialize, Deserialize, PartialEq, Debug)]
        struct TestData {
            name: String,
            value: i32,
        }

        let original = TestData {
            name: "test".to_string(),
            value: 42,
        };
        let payload = Payload::from_json(&original).expect("serialize payload");
        let restored: TestData = payload.as_json().expect("deserialize payload");
        assert_eq!(original, restored);
    }

    #[test]
    fn payload_len_and_empty() {
        let empty_payload = Payload::new(vec![]);
        assert!(empty_payload.is_empty());
        assert_eq!(empty_payload.len(), 0);

        let payload = Payload::new(vec![1, 2, 3]);
        assert!(!payload.is_empty());
        assert_eq!(payload.len(), 3);
    }

    #[test]
    fn bounding_box_contains() {
        let bbox = BoundingBox::new(Point3D::new(0.0, 0.0, 0.0), Point3D::new(1.0, 1.0, 1.0));
        assert!(bbox.contains(&Point3D::new(0.5, 0.5, 0.5)));
        assert!(bbox.contains(&Point3D::new(0.0, 0.0, 0.0)));
        assert!(bbox.contains(&Point3D::new(1.0, 1.0, 1.0)));
        assert!(!bbox.contains(&Point3D::new(1.5, 0.5, 0.5)));
    }

    #[test]
    fn bounding_box_intersects() {
        let a = BoundingBox::new(Point3D::new(0.0, 0.0, 0.0), Point3D::new(1.0, 1.0, 1.0));
        let b = BoundingBox::new(Point3D::new(0.5, 0.5, 0.5), Point3D::new(1.5, 1.5, 1.5));
        let c = BoundingBox::new(Point3D::new(2.0, 2.0, 2.0), Point3D::new(3.0, 3.0, 3.0));
        assert!(a.intersects(&b));
        assert!(!a.intersects(&c));
    }

    #[test]
    fn bounding_box_center() {
        let bbox = BoundingBox::new(Point3D::new(0.0, 0.0, 0.0), Point3D::new(2.0, 4.0, 6.0));
        let c = bbox.center();
        assert!((c.x - 1.0).abs() < 1e-6);
        assert!((c.y - 2.0).abs() < 1e-6);
        assert!((c.z - 3.0).abs() < 1e-6);
    }

    #[test]
    fn distance_metrics_cosine_orthogonal() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let c = vec![1.0, 0.0];

        let cosine_ab = DistanceMetric::Cosine.compute(&a, &b);
        assert!(cosine_ab.abs() < f32::EPSILON);
        let cosine_ac = DistanceMetric::Cosine.compute(&a, &c);
        assert!((cosine_ac - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn distance_metric_euclidean() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        let d = DistanceMetric::Euclidean.compute(&a, &b);
        assert!((d - std::f32::consts::SQRT_2).abs() < 1e-5);
    }

    #[test]
    fn distance_metric_dot_product() {
        let a = vec![2.0, 3.0];
        let b = vec![4.0, 5.0];
        // 2*4 + 3*5 = 23
        let d = DistanceMetric::DotProduct.compute(&a, &b);
        assert!((d - 23.0).abs() < f32::EPSILON);
    }

    #[test]
    fn edge_weight_default_and_override() {
        let e = Edge::new(
            EdgeId::new(1),
            EntityId::new(10),
            EntityId::new(20),
            "knows".to_string(),
        );
        assert!((e.weight - 1.0).abs() < f32::EPSILON);
        let e2 = e.with_weight(0.42);
        assert!((e2.weight - 0.42).abs() < f32::EPSILON);
    }

    #[test]
    fn document_builder_with_entities_and_location() {
        let entity = Entity::new(EntityId::new(1), "Alice".to_string(), "Person".to_string());
        let doc = Document::new(DocId::new(1), Embedding::new(vec![1.0]), Payload::default())
            .with_entities(vec![entity])
            .with_location(Point3D::new(1.0, 2.0, 3.0));
        assert_eq!(doc.entities.len(), 1);
        assert!(doc.location.is_some());
    }

    #[test]
    fn qualified_doc_id_key_format() {
        let ns = NamespaceId::new("prod");
        let doc_id = DocId::new(123);
        let qid = QualifiedDocId::new(ns.clone(), doc_id);

        let key_bytes = qid.to_key_bytes();
        let mut expected = Vec::new();
        expected.extend_from_slice(&4u32.to_le_bytes());
        expected.extend_from_slice(b"prod");
        expected.push(b'd');
        expected.extend_from_slice(&123u64.to_le_bytes());
        assert_eq!(key_bytes, expected);
    }

    // ── Property-based tests ─────────────────────────────────────────────────

    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_doc_id_roundtrip(id in any::<u64>()) {
            let did = DocId::new(id);
            assert_eq!(did.inner(), id);
            let did2: DocId = id.into();
            assert_eq!(did2, did);
        }

        #[test]
        fn prop_cosine_symmetry(
            a0 in -1.0f32..1.0,
            a1 in -1.0f32..1.0,
            b0 in -1.0f32..1.0,
            b1 in -1.0f32..1.0,
        ) {
            // cosine(a,b) == cosine(b,a)
            let a = vec![a0, a1];
            let b = vec![b0, b1];
            let ab = DistanceMetric::Cosine.compute(&a, &b);
            let ba = DistanceMetric::Cosine.compute(&b, &a);
            prop_assert!((ab - ba).abs() < 1e-5, "cosine not symmetric: {} vs {}", ab, ba);
        }

        #[test]
        fn prop_bounding_box_center_inside(
            x0 in -100.0f32..-1.0,
            y0 in -100.0f32..-1.0,
            z0 in -100.0f32..-1.0,
            x1 in 1.0f32..100.0,
            y1 in 1.0f32..100.0,
            z1 in 1.0f32..100.0,
        ) {
            let bbox = BoundingBox::new(
                Point3D::new(x0, y0, z0),
                Point3D::new(x1, y1, z1),
            );
            prop_assert!(bbox.contains(&bbox.center()), "center should always be contained");
        }

        #[test]
        fn prop_embedding_normalize_idempotent(
            v0 in -1.0f32..1.0,
            v1 in -1.0f32..1.0,
            v2 in -1.0f32..1.0,
        ) {
            // Normalization of a non-zero vector is idempotent
            if v0 * v0 + v1 * v1 + v2 * v2 > 1e-6 {
                let emb = Embedding::new(vec![v0, v1, v2]);
                let n1 = emb.normalize();
                let n2 = n1.normalize();
                for (a, b) in n1.as_slice().iter().zip(n2.as_slice()) {
                    prop_assert!((a - b).abs() < 1e-5, "normalize not idempotent");
                }
            }
        }
    }
}

// ═══ DISTRIBUTED TYPES / SPEC-034 §3.2, SPEC-036, SPEC-037 ═══

/// Unique identifier for a physical node in the cluster.
///
/// 128-bit UUID ensures global uniqueness across swarm reconfiguration.
/// Used as the authoritative key in membership tables, shard manifests,
/// and anti-affinity placement.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq, Eq, Hash))]
pub struct ClusterNodeId(pub [u8; 16]);

impl ClusterNodeId {
    /// Creates a new `ClusterNodeId` from raw bytes.
    #[inline]
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes of this node ID.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for ClusterNodeId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Node({:02x}{:02x}…{:02x}{:02x})",
            self.0[0], self.0[1], self.0[14], self.0[15]
        )
    }
}

/// Identifier for a BFT consensus proposal.
///
/// Each proposal represents a single belief-state mutation that requires
/// `2f + 1` valid votes before commitment to the Raft log.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq, Eq, Hash))]
pub struct ProposalId(pub [u8; 16]);

impl ProposalId {
    /// Creates a new `ProposalId` from raw bytes.
    #[inline]
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes of this proposal ID.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for ProposalId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Proposal({:02x}{:02x}…{:02x}{:02x})",
            self.0[0], self.0[1], self.0[14], self.0[15]
        )
    }
}

/// Index of a single shard within an erasure-coded snapshot.
///
/// Combined with `SnapshotId`, uniquely identifies a fragment in the cluster.
/// Indices `0..k` are data shards, `k..k+m` are parity shards.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq, Eq, Hash))]
pub struct ShardId(pub u32);

impl ShardId {
    /// Creates a new shard index.
    #[inline]
    pub const fn new(idx: u32) -> Self {
        Self(idx)
    }

    /// Returns the inner index value.
    #[inline]
    pub const fn inner(self) -> u32 {
        self.0
    }

    /// Returns `true` if this shard index refers to a data shard (not parity).
    #[inline]
    pub const fn is_data_shard(self, data_shards: u32) -> bool {
        self.0 < data_shards
    }
}

impl std::fmt::Display for ShardId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Shard({})", self.0)
    }
}

/// Identifier for an SSTable snapshot that has been erasure-coded.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    Archive,
    RkyvSerialize,
    RkyvDeserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq, Eq, Hash))]
pub struct SnapshotId(pub [u8; 16]);

impl SnapshotId {
    /// Creates a new `SnapshotId` from raw bytes.
    #[inline]
    pub const fn new(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    /// Returns the raw bytes.
    #[inline]
    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }
}

impl std::fmt::Display for SnapshotId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Snapshot({:02x}{:02x}…{:02x}{:02x})",
            self.0[0], self.0[1], self.0[14], self.0[15]
        )
    }
}

/// Tracks the placement of all shards for a single snapshot across the cluster.
///
/// # INVARIANT: Anti-Affinity
/// Every shard MUST reside on a distinct physical node. The `ShardDistributor`
/// enforces this at distribution time; the manifest records the result.
///
/// # DETERMINISM: O(1) lookups via `placement` map.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq))]
pub struct ShardManifest {
    /// The snapshot this manifest describes.
    pub snapshot_id: SnapshotId,
    /// Number of data shards (`k`).
    pub data_shards: u32,
    /// Number of parity shards (`m`).
    pub parity_shards: u32,
    /// Byte length of each individual shard.
    pub shard_size: usize,
    /// Original (unpadded) data length, for correct reconstruction trimming.
    pub original_data_len: usize,
    /// blake3 hash of the original data before encoding.
    pub data_hash: [u8; 32],
    /// Mapping: `ShardId → ClusterNodeId` indicating where each shard is stored.
    pub placement: Vec<ShardPlacement>,
}

/// Placement record for a single shard.
#[derive(Debug, Clone, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq))]
pub struct ShardPlacement {
    /// Index of this shard (0..k+m).
    pub shard_id: ShardId,
    /// Node that holds this shard.
    pub node_id: ClusterNodeId,
    /// blake3 hash of the shard content for integrity verification.
    pub shard_hash: [u8; 32],
}

/// Status of a BFT proposal in the voting pipeline.
#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Archive, RkyvSerialize, RkyvDeserialize,
)]
#[archive_attr(derive(bytecheck::CheckBytes, Debug, PartialEq, Eq))]
pub enum ProposalStatus {
    /// Votes are still being collected.
    Pending,
    /// Quorum (`2f + 1`) has been reached — ready for Raft commit.
    Quorum,
    /// Proposal has been committed to the Raft log.
    Committed,
    /// Proposal timed out before reaching quorum.
    TimedOut,
}

impl std::fmt::Display for ProposalStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::Quorum => write!(f, "quorum"),
            Self::Committed => write!(f, "committed"),
            Self::TimedOut => write!(f, "timed_out"),
        }
    }
}

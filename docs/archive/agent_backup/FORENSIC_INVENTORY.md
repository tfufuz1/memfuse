# FORENSIC INVENTORY

## memfuse-core
### Traits
- `pub trait Checkpoint: Send + Sync {`
- `pub trait Snapshot: Send + Sync {`
- `pub trait StorageEngine: Send + Sync {`
- `pub trait VectorIndex: Send + Sync {`
- `pub trait TextIndex: Send + Sync {`
- `pub trait GraphIndex: Send + Sync {`
### Structs
- `pub struct WorkflowState {`
- `pub struct DocId(pub u64);`
- `pub struct EntityId(pub u64);`
- `pub struct TxId(pub u64);`
- `pub struct Embedding {`
- `pub struct ScoredDocument {`
- `pub struct Entity {`
- `pub struct Edge {`
- `pub struct VectorIndexStats {`
- `pub struct StorageStats {`
- `pub struct TextIndexStats {`
- `pub struct GraphIndexStats {`
- `pub struct ResourceBudget {`
- `pub struct ResourceTracker {`
- `pub struct NamespaceId(u64);`
- `pub struct TokenBudget {`
- `pub struct FusionWeights {`
- `pub struct ContextChunk {`
- `pub struct ContextWindow {`
- `pub struct ScoredEntry {`
- `pub struct HybridQuery {`
- `pub struct HybridQueryBuilder {`
- `pub struct TxBuffer<T: Clone> {`
- `pub struct SnapshotRegistry {`
- `pub struct SnapshotGuard {`
### Enums
- `pub enum IsolationLevel {`
- `pub enum FilterExpr {`
- `pub enum DistanceMetric {`
- `pub enum MemFuseError {`
- `pub enum IndexOp<T: Clone> {`
### Todos & Unimplemented
### Empty Functions
- `async fn pin_checkpoint(&self, _seq_no: u64) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn unpin_checkpoint(&self, _seq_no: u64) -> Result<()> {`
- `Ok(())`
- `}`
### Allow Dead Code
### Clippy Warnings


## memfuse-store
### Traits
### Structs
- `pub struct LsmConfig {`
- `pub struct LsmStorage {`
- `pub struct WalEntry {`
- `pub struct Wal {`
- `pub struct MmapReader {`
- `pub struct BloomFilter {`
- `pub struct BlockBuilder {`
- `pub struct SstableMetadata {`
- `pub struct SstableBuilder {`
- `pub struct SstableReader {`
- `pub struct CompactionConfig {`
- `pub struct CompactionEngine {`
- `pub struct MemTable {`
- `pub struct StateCheckpoint {`
- `pub struct Checkpointer {`
### Enums
- `pub enum WalOp {`
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
### Clippy Warnings


## memfuse-index
### Traits
### Structs
- `pub struct HnswHeader {`
- `pub struct NodeRecord {`
- `pub struct MmapIndex {`
- `pub struct HnswConfig {`
- `pub struct HnswIndex {`
- `pub struct HnswIndexCore {`
- `pub struct ScalarQuantizer {`
- `pub struct CosineSimilarityPartsU8 {`
- `pub struct CosineSimilarityPartsF32U8 {`
- `pub struct DiskAnnConfig {`
- `pub struct DiskAnnIndex {`
### Enums
- `pub enum VectorData {`
### Todos & Unimplemented
### Empty Functions
- `async fn commit(&self, _tx: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn rollback(&self, _tx: TxId) -> Result<()> {`
- `Ok(())`
- `}`
### Allow Dead Code
### Clippy Warnings


## memfuse-text
### Traits
- `pub trait Tokenizer: Send + Sync {`
- `pub trait MorphologicalTokenizer: Send + Sync {`
### Structs
- `pub struct DefaultTokenizer;`
- `pub struct GermanMorphTokenizer {`
- `pub struct GermanCompoundSplitter {`
- `pub struct PassthroughTokenizer {`
- `pub struct TokenReductionMetrics {`
- `pub struct Bm25Scorer<S: memfuse_core::StorageEngine> {`
- `pub struct InvertedIndex<S: StorageEngine> {`
- `pub struct BM25MorphIndex<S: StorageEngine> {`
### Enums
### Todos & Unimplemented
### Empty Functions
- `async fn commit(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn rollback(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn flush(&self) -> Result<()> {`
- `Ok(())`
- `}`
### Allow Dead Code
### Clippy Warnings


## memfuse-crypto
### Traits
- `pub trait KmsProvider {`
### Structs
- `pub struct EncryptedWal {`
- `pub struct WalHmac {`
- `pub struct WalEntrySnapshot {`
- `pub struct IntegrityVerifier {`
- `pub struct KeyManager {`
### Enums
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
### Clippy Warnings


## memfuse-graph
### Traits
### Structs
- `pub struct CsrGraph {`
### Enums
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
### Clippy Warnings


## memfuse-db
### Traits
### Structs
- `pub struct ContextManager {`
- `pub struct SpatialFence {`
- `pub struct ChunkerConfig {`
- `pub struct MarkdownChunker {`
- `pub struct SearchResult {`
- `pub struct DbStats {`
- `pub struct Document {`
- `pub struct MemFuseConfig {`
- `pub struct MemFuse {`
- `pub struct DbTransaction<'a, S: StorageEngine> {`
- `pub struct Collection<S: StorageEngine = LsmStorage> {`
- `pub struct Namespace {`
- `pub struct NamespaceHandle {`
- `pub struct NamespaceRegistry {`
### Enums
- `pub enum FilterOp {`
- `pub enum MetadataFilter {`
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
- `#[allow(dead_code)]` used
### Clippy Warnings


## memfuse-py
### Traits
### Structs
- `pub struct PySearchResult {`
- `pub struct PyDocument {`
- `pub struct PyVectorIndexStats {`
- `pub struct PyStorageStats {`
- `pub struct PyDbStats {`
- `pub struct PyMemFuse {`
- `pub struct PyCollection {`
### Enums
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
### Clippy Warnings


## memfuse-sandbox
### Traits
- `pub trait AgentRuntime: Send + Sync {`
### Structs
- `pub struct SandboxState {`
- `pub struct AirGapConfig {`
- `pub struct AirGapVerifier;`
- `pub struct AirGapReport {`
- `pub struct SandboxConfig {`
- `pub struct WasmSandbox {`
### Enums
- `pub enum EmbeddingRuntime {`
### Todos & Unimplemented
### Empty Functions
### Allow Dead Code
### Clippy Warnings


## memfuse-checkpoint
### Traits
### Structs
- `pub struct CheckpointMeta {`
- `pub struct CheckpointRegistry {`
- `pub struct PersistentCheckpointStore<S: StorageEngine> {`
### Enums
### Todos & Unimplemented
### Empty Functions
- `async fn commit(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn rollback(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn flush(&self) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn commit(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn rollback(&self, _tx_id: TxId) -> Result<()> {`
- `Ok(())`
- `}`
- `async fn flush(&self) -> Result<()> {`
- `Ok(())`
- `}`
### Allow Dead Code
### Clippy Warnings


## memfuse-saos-agent
### Traits
- `pub trait AgentTool: Send + Sync {`
### Structs
- `pub struct AuditEntry {`
- `pub struct AuditLog {`
- `pub struct OrchestratorEngine {`
- `pub struct StepResult {`
- `pub struct AgentContext {`
- `pub struct AgentNode {`
- `pub struct WorkflowEdge {`
- `pub struct StateGraph {`
### Enums
- `pub enum NodeType {`
### Todos & Unimplemented
### Empty Functions
- `async fn persist_final_state(&self, _ctx: &AgentContext) -> Result<()> {`
- `Ok(())`
- `}`
### Allow Dead Code
### Clippy Warnings


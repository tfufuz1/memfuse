# FORENSIC INVENTORY

## Crate: memfuse-core

### Public Traits
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:20:pub trait Checkpoint: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:29:pub trait Snapshot: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:63:pub trait StorageEngine: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:131:pub trait VectorIndex: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:198:pub trait TextIndex: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:224:pub trait GraphIndex: Send + Sync {
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:36:pub struct VectorIndexStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:47:pub struct StorageStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:187:pub struct TextIndexStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/traits.rs:256:pub struct GraphIndexStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs:65:pub struct TxBuffer<T: Clone> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:28:pub struct SnapshotRegistry {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:102:pub struct SnapshotGuard {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:8:pub struct NamespaceId(u64);
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:28:pub struct TokenBudget {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:73:pub struct FusionWeights {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:117:pub struct ContextChunk {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:127:pub struct ContextWindow {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:135:pub struct ScoredEntry {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:143:pub struct HybridQuery {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:160:pub struct HybridQueryBuilder {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:6:pub struct WorkflowState {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:20:pub struct DocId(pub u64);
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:65:pub struct EntityId(pub u64);
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:94:pub struct TxId(pub u64);
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:130:pub struct Embedding {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:164:pub struct ScoredDocument {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:177:pub struct Entity {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:195:pub struct Edge {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:5:pub struct ResourceBudget {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:20:pub struct ResourceTracker {
```

### Public Enums
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/error.rs:17:pub enum MemFuseError {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs:25:pub enum IndexOp<T: Clone> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:109:pub enum IsolationLevel {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/filter.rs:5:pub enum FilterExpr {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:121:pub enum DistanceMetric {
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs:215:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/tx_buffer.rs:231:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:228:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:235:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:246:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:262:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/saos.rs:273:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:125:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:138:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:151:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:167:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/snapshot.rs:178:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:222:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/domain.rs:237:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:119:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:135:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:153:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:165:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:190:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-core/src/types/budget.rs:210:    #[test]
```

## Crate: memfuse-store

### Public Traits
```
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:49:pub struct BloomFilter {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:151:pub struct BlockBuilder {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:223:pub struct SstableMetadata {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:232:pub struct SstableBuilder {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:404:pub struct SstableReader {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:966:pub struct SstableStream {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:17:pub struct MemTable {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:70:pub struct LsmConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:104:pub struct LsmStorage {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:33:pub struct CompactionConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:59:pub struct CompactionEngine {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/checkpoint.rs:12:pub struct StateCheckpoint {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/checkpoint.rs:18:pub struct Checkpointer {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/mmap.rs:11:pub struct MmapReader {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:34:pub struct WalEntry {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:140:pub struct Wal {
```

### Public Enums
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:12:pub enum WalOp {
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/tests/rollback_sstables.rs:6:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1360:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1412:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1434:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1465:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1503:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1531:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/sstable.rs:1573:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/tests/encryption_test.rs:6:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/tests/encryption_test.rs:44:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/tests/encryption_test.rs:75:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:162:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:179:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:205:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:219:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/memtable.rs:240:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/checkpoint.rs:57:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:406:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:448:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:481:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:513:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:573:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/compaction.rs:736:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:939:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:951:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:967:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:979:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:986:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:1002:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:1048:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:1097:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/lsm.rs:1150:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:670:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:691:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:721:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:754:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:783:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-store/src/wal.rs:821:    #[tokio::test]
```

## Crate: memfuse-index

### Public Traits
```
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:114:pub struct DiskAnnConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:169:pub struct DiskAnnIndex {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs:15:pub struct ScalarQuantizer {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs:578:pub struct CosineSimilarityPartsU8 {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs:649:pub struct CosineSimilarityPartsF32U8 {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/persistence.rs:22:pub struct HnswHeader {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/persistence.rs:133:pub struct NodeRecord {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/persistence.rs:179:pub struct MmapIndex {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:57:pub struct HnswConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:160:pub struct HnswIndex {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:172:pub struct HnswIndexCore {
```

### Public Enums
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:112:pub enum VectorData {
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:760:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:774:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:802:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/diskann.rs:836:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/tests/poisoning.rs:5:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/tests/poisoning.rs:23:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/tests/poisoning.rs:41:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/tests/ram_reduction.rs:4:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1697:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1713:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1742:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1762:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1776:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1786:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1794:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1824:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1874:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1923:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1989:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/hnsw.rs:1998:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/tests/recall.rs:5:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs:202:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs:226:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/quantize.rs:261:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs:894:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-index/src/distance.rs:916:    #[test]
```

## Crate: memfuse-text

### Public Traits
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:25:pub trait Tokenizer: Send + Sync {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:14:pub trait MorphologicalTokenizer: Send + Sync {
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:31:pub struct DefaultTokenizer;
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:44:pub struct GermanMorphTokenizer {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:22:pub struct TextIndexMetadata {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:29:pub struct InvertedIndex<S: StorageEngine> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:366:pub struct BM25MorphIndex<S: StorageEngine> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/lib.rs:20:pub struct Bm25Scorer<S: memfuse_core::StorageEngine> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:26:pub struct GermanCompoundSplitter {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:118:pub struct PassthroughTokenizer {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:141:pub struct TokenReductionMetrics {
```

### Public Enums
```
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:500:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:549:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:605:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:641:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/inverted.rs:678:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:54:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:60:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:66:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:73:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:80:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/bm25.rs:90:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:166:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:175:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/morphology.rs:182:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:100:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:108:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-text/src/tokenizer.rs:116:    #[test]
```

## Crate: memfuse-crypto

### Public Traits
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:12:pub trait KmsProvider {
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:18:pub struct EncryptedWal {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:46:pub struct WalHmac {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:68:pub struct WalEntrySnapshot {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:78:pub struct IntegrityVerifier {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:16:pub struct KeyManager {
```

### Public Enums
```
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:106:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:118:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:128:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:141:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:150:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/crypto.rs:160:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:123:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:132:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/src/wal_crypto.rs:188:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/tests/nonce_reuse.rs:3:#[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-crypto/tests/nonce_reuse.rs:22:#[test]
```

## Crate: memfuse-graph

### Public Traits
```
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:138:pub struct CsrGraph {
```

### Public Enums
```
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:390:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:429:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:449:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:477:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-graph/src/csr.rs:498:    #[tokio::test]
```

## Crate: memfuse-db

### Public Traits
```
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:12:pub struct ChunkerConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:35:pub struct MarkdownChunker {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/transaction.rs:17:pub struct DbTransaction<'a, S: StorageEngine> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs:25:pub struct ContextManager {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs:108:pub struct SpatialFence {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:15:pub struct Namespace {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:45:pub struct NamespaceHandle {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:74:pub struct NamespaceRegistry {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/collection.rs:53:pub struct Collection<S: StorageEngine = LsmStorage> {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:72:pub struct SearchResult {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:83:pub struct DbStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:92:pub struct Document {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:101:pub struct MemFuseConfig {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:128:pub struct MemFuse {
```

### Public Enums
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/filter.rs:6:pub enum FilterOp {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/filter.rs:27:pub enum MetadataFilter {
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:47:    #[allow(dead_code)]
```

### Tests
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/reaper.rs:42:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/text_integration.rs:7:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:235:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:262:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:282:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/chunker.rs:301:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/collection_contract.rs:26:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/collection_contract.rs:50:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/collection_contract.rs:79:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/collection_contract.rs:122:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/transaction_isolation.rs:38:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/transaction_isolation.rs:70:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/transaction_isolation.rs:137:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:614:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:644:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:664:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:684:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:706:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:723:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:740:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:747:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:754:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:798:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:811:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:876:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:906:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:935:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:950:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:964:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/lib.rs:979:    #[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/full_stack_e2e.rs:8:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/layer_bounds.rs:10:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/layer_bounds.rs:68:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/filter_tests.rs:17:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/filter_tests.rs:49:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/filter_tests.rs:83:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:182:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/namespace.rs:194:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/checkpoint_layer_bounds.rs:14:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/fusion.rs:52:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/fusion.rs:91:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/fusion.rs:106:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/fusion.rs:133:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/tests/atomic_commit.rs:4:#[tokio::test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs:136:    #[test]
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-db/src/context.rs:175:    #[test]
```

## Crate: memfuse-py

### Public Traits
```
```

### Public Structs
```
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:120:pub struct PySearchResult {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:138:pub struct PyDocument {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:155:pub struct PyVectorIndexStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:177:pub struct PyStorageStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:199:pub struct PyDbStats {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:486:pub struct PyMemFuse {
/home/freddy/Arbeitsplatz/DEV/memfuse/crates/memfuse-py/src/lib.rs:592:pub struct PyCollection {
```

### Public Enums
```
```

### Skeletons (todo/unimplemented)
```
```

### Dead Code Allowances
```
```

### Tests
```
```


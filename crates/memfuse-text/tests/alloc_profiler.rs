//! Allocation Profiling Harness for memfuse-text
//! Measures heap allocation count, bytes allocated, execution time, and throughput
//! for 10,000 documents (~500 words per document) across German and English workloads.

use memfuse_core::{
    BoxFuture, DocId, StorageEngine, TextIndex, TxId};
use memfuse_text::inverted::{InvertedIndex, Language};
use memfuse_text::tokenizer::{DefaultTokenizer, GermanMorphTokenizer, Tokenizer};
use std::alloc::{GlobalAlloc, Layout, System};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use unicode_segmentation::UnicodeSegmentation;

struct CountingAllocator;

static ALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static DEALLOC_COUNT: AtomicU64 = AtomicU64::new(0);
static ALLOC_BYTES: AtomicU64 = AtomicU64::new(0);

unsafe impl GlobalAlloc for CountingAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        ALLOC_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        System.alloc(layout)
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        DEALLOC_COUNT.fetch_add(1, Ordering::Relaxed);
        System.dealloc(ptr, layout);
    }
}

#[global_allocator]
static A: CountingAllocator = CountingAllocator;

fn reset_stats() {
    ALLOC_COUNT.store(0, Ordering::Relaxed);
    DEALLOC_COUNT.store(0, Ordering::Relaxed);
    ALLOC_BYTES.store(0, Ordering::Relaxed);
}

fn get_stats() -> (u64, u64, u64) {
    (
        ALLOC_COUNT.load(Ordering::Relaxed),
        DEALLOC_COUNT.load(Ordering::Relaxed),
        ALLOC_BYTES.load(Ordering::Relaxed),
    )
}

use std::collections::BTreeMap;

struct FastRamStorage {
    store: parking_lot::RwLock<BTreeMap<Vec<u8>, Vec<u8>>>,
}

impl FastRamStorage {
    fn new() -> Self {
        Self {
            store: parking_lot::RwLock::new(BTreeMap::new()),
        }
    }
}


impl StorageEngine for FastRamStorage {
    fn get<'a>(&'a self, key: &'a [u8]) -> BoxFuture<'a, memfuse_core::Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        Ok(self.store.read().get(key).cloned())
        })
    }
    fn put<'a>(&'a self, _tx_id: TxId, key: &'a [u8], value: &'a [u8]) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        self.store.write().insert(key.to_vec(), value.to_vec());
        Ok(())
        })
    }
    fn delete<'a>(&'a self, _tx_id: TxId, key: &'a [u8]) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        self.store.write().remove(key);
        Ok(())
        })
    }
    fn commit<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn rollback<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn rollback_to_tx<'a>(&'a self, _tx_id: TxId) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn get_at_seq<'a>(&'a self, key: &'a [u8], _seq: u64) -> BoxFuture<'a, memfuse_core::Result<Option<Vec<u8>>>> {
        Box::pin(async move {
        Ok(self.store.read().get(key).cloned())
        })
    }
    fn last_seq_no<'a>(&'a self) -> BoxFuture<'a, memfuse_core::Result<u64>> {
        Box::pin(async move {
        Ok(1)
        })
    }
    fn last_tx_id<'a>(&'a self) -> BoxFuture<'a, memfuse_core::Result<TxId>> {
        Box::pin(async move {
        Ok(TxId::new(1))
        })
    }
    fn flush<'a>(&'a self) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn stats<'a>(&'a self) -> BoxFuture<'a, memfuse_core::Result<memfuse_core::StorageStats>> {
        Box::pin(async move {
        Ok(memfuse_core::StorageStats {
            num_segments: 1,
            total_size_bytes: 0,
            memtable_size_bytes: 0,
        })
        })
    }
    fn pin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn unpin_checkpoint<'a>(&'a self, _id: u64) -> BoxFuture<'a, memfuse_core::Result<()>> {
        Box::pin(async move {
        Ok(())
        })
    }
    fn scan<'a>(
        &'a self,
        _start: std::ops::Bound<&'a [u8]>,
        _end: std::ops::Bound<&'a [u8]>,
    ) -> BoxFuture<'a, memfuse_core::Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        Ok(Vec::new())
        })
    }
    fn scan_prefix<'a>(&'a self, prefix: &'a [u8]) -> BoxFuture<'a, memfuse_core::Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        let guard = self.store.read();
        let res = guard
            .range(prefix.to_vec()..)
            .take_while(|(k, _)| k.starts_with(prefix))
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        Ok(res)
        })
    }
    fn scan_prefix_at<'a>(
        &'a self,
        prefix: &'a [u8],
        _seq_no: u64,
    ) -> BoxFuture<'a, memfuse_core::Result<Vec<(Vec<u8>, Vec<u8>)>>> {
        Box::pin(async move {
        self.scan_prefix(prefix).await
        })
    }
}

/// Generates a realistic German text document with ~500 words.
fn generate_german_doc(doc_idx: usize) -> String {
    let sentence_templates = [
        "Das Bundesverfassungsgericht entschied über die Datenschutzrichtlinie und die Sicherheitsarchitektur der Informationstechnik im Mittelstand.",
        "Der Arbeitsvertrag regelt das Gehalt, die Urlaubsansprüche und die Arbeitszeit für die Mitarbeiter im Unternehmen.",
        "Im Qualitätsmanagementsystem sind alle Prozesse zur Auftragsbestätigung, Rechnungsstellung und Qualitätsprüfung genau dokumentiert.",
        "Die Gesellschafsführung plant die Umstellung auf cloudbasierte Vektorsuche und hybrides Suchmaschinen-Indexing.",
        "Ein Urlaubsantragsprozess erfordert die Abstimmung zwischen Abteilungsleiter, Personalabteilung und Betriebsrat.",
        "Für die Softwareentwicklung werden moderne Programmiersprachen wie Rust für hohe Performance und Speichersicherheit verwendet.",
        "Die Lagerbestandsverwaltung optimiert die Lieferkette, reduziert die Verwaltungskosten und verbessert das Zahlungsziel.",
        "Im Beratungsgespräch wurden Fragen zur Haftungsbeschränkung, Risikobewertung und Vertragsgestaltung ausführlich erörtert.",
        "Die Sicherheitsüberprüfung der IT-Infrastruktur verhinderte potenzielle Datenlecks und stärkte das Vertrauen der Geschäftspartner.",
        "Durch die kontinuierliche Wartungsarbeit wird der unterbrechungsfreie Betrieb der Produktionsanlagen sichergestellt.",
    ];

    let mut doc = String::with_capacity(3500);
    doc.push_str(&format!("Dokument ID {} - ", doc_idx));
    for i in 0..35 {
        let template = sentence_templates[(doc_idx + i) % sentence_templates.len()];
        doc.push_str(template);
        doc.push(' ');
    }
    doc
}

/// Generates a realistic English text document with ~500 words.
fn generate_english_doc(doc_idx: usize) -> String {
    let sentence_templates = [
        "The Supreme Court issued a landmark ruling regarding data privacy regulations and enterprise information security standards.",
        "The employment contract specifies salary compensation, paid time off policies, and working hours for technical staff.",
        "Under the quality management system, procedures for order confirmation, invoicing, and quality assurance are rigorously defined.",
        "Executive management plans the transition to cloud-native vector search and hybrid search engine indexing architectures.",
        "The vacation request workflow requires coordination between team leads, human resources, and department heads.",
        "Modern systems programming languages like Rust offer high throughput, concurrency safety, and zero-cost abstractions.",
        "Inventory management optimizes the supply chain, minimizes administrative overhead, and improves working capital efficiency.",
        "During the consultation session, key aspects of legal liability, risk assessment, and contract terms were analyzed in detail.",
        "Regular security audits of the IT infrastructure prevent potential vulnerabilities and bolster partner trust.",
        "Proactive maintenance protocols guarantee continuous operation and high availability for manufacturing hardware.",
    ];

    let mut doc = String::with_capacity(3500);
    doc.push_str(&format!("Document ID {} - ", doc_idx));
    for i in 0..35 {
        let template = sentence_templates[(doc_idx + i) % sentence_templates.len()];
        doc.push_str(template);
        doc.push(' ');
    }
    doc
}

#[tokio::test]
#[ignore]
async fn run_profile_10k_documents() {
    println!("\n================================================================================");
    println!("MEMFUSE-TEXT ALLOCATION PROFILE & BENCHMARK HARNESS (10,000 DOCS)");
    println!("================================================================================");

    const NUM_DOCS: usize = 10_000;

    // --- PHASE 1: Tokenizer direct evaluation (Isolation) ---
    println!("\n--- [1] TOKENIZER DIRECT EVALUATION (10,000 Documents) ---");

    // 1a. German Morphological Tokenizer
    let german_tok = GermanMorphTokenizer::new();
    let mut total_words_de = 0usize;
    let mut total_tokens_de = 0usize;

    // Generate documents first so doc creation allocations aren't counted
    let german_docs: Vec<String> = (0..NUM_DOCS).map(generate_german_doc).collect();
    for doc in &german_docs {
        total_words_de += doc.unicode_words().count();
    }

    reset_stats();
    let start_de_tok = Instant::now();
    for doc in &german_docs {
        let tokens = german_tok.tokenize(doc);
        total_tokens_de += tokens.len();
    }
    let dur_de_tok = start_de_tok.elapsed();
    let (allocs_de_tok, _deallocs_de_tok, bytes_de_tok) = get_stats();

    println!("GERMAN TOKENIZER:");
    println!("  - Documents processed   : {}", NUM_DOCS);
    println!("  - Total words           : {}", total_words_de);
    println!(
        "  - Avg words/doc         : {:.1}",
        total_words_de as f64 / NUM_DOCS as f64
    );
    println!("  - Total tokens generated: {}", total_tokens_de);
    println!("  - Total Allocations     : {}", allocs_de_tok);
    println!(
        "  - Total Bytes Allocated : {} ({:.2} MB)",
        bytes_de_tok,
        bytes_de_tok as f64 / 1_048_576.0
    );
    println!(
        "  - Allocations / Word    : {:.4}",
        allocs_de_tok as f64 / total_words_de as f64
    );
    println!(
        "  - Allocations / Token   : {:.4}",
        allocs_de_tok as f64 / total_tokens_de as f64
    );
    println!(
        "  - Time Elapsed          : {:.3} s",
        dur_de_tok.as_secs_f64()
    );
    println!(
        "  - Tokenizer Throughput  : {:.1} docs/s, {:.1} words/s",
        NUM_DOCS as f64 / dur_de_tok.as_secs_f64(),
        total_words_de as f64 / dur_de_tok.as_secs_f64()
    );

    // 1b. English / Default Tokenizer
    let english_tok = DefaultTokenizer;
    let english_docs: Vec<String> = (0..NUM_DOCS).map(generate_english_doc).collect();
    let mut total_words_en = 0usize;
    let mut total_tokens_en = 0usize;
    for doc in &english_docs {
        total_words_en += doc.unicode_words().count();
    }

    reset_stats();
    let start_en_tok = Instant::now();
    for doc in &english_docs {
        let tokens = english_tok.tokenize(doc);
        total_tokens_en += tokens.len();
    }
    let dur_en_tok = start_en_tok.elapsed();
    let (allocs_en_tok, _deallocs_en_tok, bytes_en_tok) = get_stats();

    println!("\nENGLISH DEFAULT TOKENIZER:");
    println!("  - Documents processed   : {}", NUM_DOCS);
    println!("  - Total words           : {}", total_words_en);
    println!(
        "  - Avg words/doc         : {:.1}",
        total_words_en as f64 / NUM_DOCS as f64
    );
    println!("  - Total tokens generated: {}", total_tokens_en);
    println!("  - Total Allocations     : {}", allocs_en_tok);
    println!(
        "  - Total Bytes Allocated : {} ({:.2} MB)",
        bytes_en_tok,
        bytes_en_tok as f64 / 1_048_576.0
    );
    println!(
        "  - Allocations / Word    : {:.4}",
        allocs_en_tok as f64 / total_words_en as f64
    );
    println!(
        "  - Allocations / Token   : {:.4}",
        allocs_en_tok as f64 / total_tokens_en as f64
    );
    println!(
        "  - Time Elapsed          : {:.3} s",
        dur_en_tok.as_secs_f64()
    );
    println!(
        "  - Tokenizer Throughput  : {:.1} docs/s, {:.1} words/s",
        NUM_DOCS as f64 / dur_en_tok.as_secs_f64(),
        total_words_en as f64 / dur_en_tok.as_secs_f64()
    );

    // --- PHASE 2: Full Inverted Index Ingestion Workload (Index + Storage) ---
    println!("\n--- [2] FULL INVERTED INDEX WORKLOAD (10,000 Documents) ---");

    // 2a. German Index Pipeline
    let storage_de = Arc::new(FastRamStorage::new());
    let index_de =
        InvertedIndex::new_with_language(storage_de.clone(), "de_bench", Language::German);

    reset_stats();
    let start_de_idx = Instant::now();
    for (i, doc) in german_docs.iter().enumerate() {
        let tx = TxId::new((i + 1) as u64);
        let doc_id = DocId::new((i + 1) as u64);
        index_de
            .insert(tx, doc_id, doc)
            .await
            .expect("insert succeeds");
        index_de.commit(tx).await.expect("commit succeeds");
    }
    let dur_de_idx = start_de_idx.elapsed();
    let (allocs_de_idx, _deallocs_de_idx, bytes_de_idx) = get_stats();

    println!("GERMAN FULL INDEX PIPELINE:");
    println!("  - Documents indexed     : {}", NUM_DOCS);
    println!("  - Total words indexed   : {}", total_words_de);
    println!("  - Total Allocations     : {}", allocs_de_idx);
    println!(
        "  - Total Bytes Allocated : {} ({:.2} MB)",
        bytes_de_idx,
        bytes_de_idx as f64 / 1_048_576.0
    );
    println!(
        "  - Allocations / Word    : {:.4}",
        allocs_de_idx as f64 / total_words_de as f64
    );
    println!(
        "  - Allocations / Doc     : {:.1}",
        allocs_de_idx as f64 / NUM_DOCS as f64
    );
    println!(
        "  - Time Elapsed          : {:.3} s",
        dur_de_idx.as_secs_f64()
    );
    println!(
        "  - Index Throughput      : {:.1} docs/s, {:.1} words/s",
        NUM_DOCS as f64 / dur_de_idx.as_secs_f64(),
        total_words_de as f64 / dur_de_idx.as_secs_f64()
    );

    // 2b. English Index Pipeline
    let storage_en = Arc::new(FastRamStorage::new());
    let index_en = InvertedIndex::new(storage_en.clone(), "en_bench");

    reset_stats();
    let start_en_idx = Instant::now();
    for (i, doc) in english_docs.iter().enumerate() {
        let tx = TxId::new((i + 1) as u64);
        let doc_id = DocId::new((i + 1) as u64);
        index_en
            .insert(tx, doc_id, doc)
            .await
            .expect("insert succeeds");
        index_en.commit(tx).await.expect("commit succeeds");
    }
    let dur_en_idx = start_en_idx.elapsed();
    let (allocs_en_idx, _deallocs_en_idx, bytes_en_idx) = get_stats();

    println!("\nENGLISH FULL INDEX PIPELINE:");
    println!("  - Documents indexed     : {}", NUM_DOCS);
    println!("  - Total words indexed   : {}", total_words_en);
    println!("  - Total Allocations     : {}", allocs_en_idx);
    println!(
        "  - Total Bytes Allocated : {} ({:.2} MB)",
        bytes_en_idx,
        bytes_en_idx as f64 / 1_048_576.0
    );
    println!(
        "  - Allocations / Word    : {:.4}",
        allocs_en_idx as f64 / total_words_en as f64
    );
    println!(
        "  - Allocations / Doc     : {:.1}",
        allocs_en_idx as f64 / NUM_DOCS as f64
    );
    println!(
        "  - Time Elapsed          : {:.3} s",
        dur_en_idx.as_secs_f64()
    );
    println!(
        "  - Index Throughput      : {:.1} docs/s, {:.1} words/s",
        NUM_DOCS as f64 / dur_en_idx.as_secs_f64(),
        total_words_en as f64 / dur_en_idx.as_secs_f64()
    );

    // --- PHASE 3: Search Query Path Evaluation (BM25) ---
    println!("\n--- [3] BM25 SEARCH QUERY EVALUATION (1,000 Queries) ---");
    let queries = [
        "Bundesverfassungsgericht Datenschutzrichtlinie",
        "Arbeitsvertrag Gehalt Urlaubsansprüche",
        "Qualitätsmanagementsystem Auftragsbestätigung",
        "Vektorsuche hybrides Indexing",
        "Lagerbestandsverwaltung Lieferkette",
    ];

    reset_stats();
    let start_search = Instant::now();
    for i in 0..1000 {
        let q = queries[i % queries.len()];
        let _ = index_de.search(q, 10).await.expect("search succeeds");
    }
    let dur_search = start_search.elapsed();
    let (allocs_search, _deallocs_search, bytes_search) = get_stats();

    println!("BM25 SEARCH (German Index, 1000 Queries):");
    println!("  - Total Allocations     : {}", allocs_search);
    println!(
        "  - Total Bytes Allocated : {} ({:.2} MB)",
        bytes_search,
        bytes_search as f64 / 1_048_576.0
    );
    println!(
        "  - Allocations / Query   : {:.1}",
        allocs_search as f64 / 1000.0
    );
    println!(
        "  - Time Elapsed          : {:.3} ms total ({:.3} ms/query)",
        dur_search.as_secs_f64() * 1000.0,
        dur_search.as_secs_f64()
    );
    println!(
        "  - Query Throughput      : {:.1} QPS",
        1000.0 / dur_search.as_secs_f64()
    );

    println!("\n================================================================================");
}

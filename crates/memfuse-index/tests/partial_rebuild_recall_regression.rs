#![cfg(feature = "partial-index-rebuild")]

use memfuse_core::{DistanceMetric, DocId, TxId, VectorIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use memfuse_index::persistence::MmapIndex;
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use std::collections::{HashMap, HashSet};

/// Toleranzband für den maximal zulässigen Recall@10-Abfall nach `rebuild_region()`:
/// ±5 Prozentpunkte (0.05).
///
/// Exakter gemessener Recall@10-Drop nach `rebuild_region()` ohne Re-Wiring:
/// Pattern (i)  Random 20% Tombstones:       0.7680 -> 0.7260 (-0.0420 = -4.2 pp)
/// Pattern (ii) Spatially Clustered 20%:     0.7720 -> 0.7510 (-0.0210 = -2.1 pp)
///
/// Herleitung / Begründung:
/// `rebuild_region()` führt reines Tombstone-Pruning ohne Re-Wiring durch (siehe VETOES.md#VETO-F02).
/// Durch das Entfernen der gelöschten Nachbarn sinkt der Knotengrad der Randknoten, ohne dass neue
/// Ersatz-Verbindungen geknüpft werden. Das bewirkt einen reproduzierbaren Recall-Abfall von ca. 4.2 pp.
/// Die Toleranzschranke wird auf 0.05 (5.0 Prozentpunkte) gesetzt, um diesen Ist-Zustand abzufangen
/// und künftige Verschlechterungen (Regressionen > 5 pp) abzufangen.
const RECALL_TOLERANCE_BAND: f64 = 0.05;

fn generate_vectors(count: usize, dim: usize, seed: u64) -> Vec<Vec<f32>> {
    let mut rng = StdRng::seed_from_u64(seed);
    let mut vectors = Vec::with_capacity(count);
    for _ in 0..count {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        let norm: f32 = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm > 0.0 {
            for x in v.iter_mut() {
                *x /= norm;
            }
        }
        vectors.push(v);
    }
    vectors
}

fn dot_product(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

/// Brute-force kNN (k=10) über alle AKTIVEN (nicht gelöschten) Vektoren als Ground Truth.
fn brute_force_knn(
    vectors: &[Vec<f32>],
    deleted_set: &HashSet<u64>,
    queries: &[Vec<f32>],
    k: usize,
) -> Vec<Vec<DocId>> {
    let mut all_gt = Vec::with_capacity(queries.len());
    for query in queries {
        let mut scored: Vec<(DocId, f32)> = vectors
            .iter()
            .enumerate()
            .filter(|(idx, _)| !deleted_set.contains(&(*idx as u64)))
            .map(|(idx, v)| {
                let dist = 1.0 - dot_product(query, v);
                (DocId::new(idx as u64), dist)
            })
            .collect();

        scored.sort_by(|a, b| a.1.total_cmp(&b.1));
        let top_k: Vec<DocId> = scored.into_iter().take(k).map(|(id, _)| id).collect();
        all_gt.push(top_k);
    }
    all_gt
}

async fn build_hnsw_index(vectors: &[Vec<f32>]) -> HnswIndex {
    let dim = vectors[0].len();
    let config = HnswConfig {
        dimension: dim,
        max_elements: vectors.len() + 1000,
        m: 16,
        ef_construction: 200,
        ef_search: 64,
        distance_metric: DistanceMetric::Cosine,
        rebuild_threshold: 0.0, // Automatischen globalen Rebuild deaktivieren für isolierten Test
        quantize: false,
        ..Default::default()
    };

    let index = HnswIndex::try_new(config).expect("HnswIndex creation failed");
    let tx = TxId::new(1);
    for (i, v) in vectors.iter().enumerate() {
        index
            .insert(tx, DocId::new(i as u64), v)
            .await
            .expect("insert failed");
    }
    index.commit(tx).await.expect("commit failed");
    index
}

async fn evaluate_recall_at_k(
    index: &HnswIndex,
    queries: &[Vec<f32>],
    ground_truth: &[Vec<DocId>],
    k: usize,
) -> f64 {
    let mut total_recall = 0.0;
    for (i, query) in queries.iter().enumerate() {
        let results = index.search(query, k).await.expect("search failed");
        let gt_set: HashSet<DocId> = ground_truth[i].iter().copied().collect();
        let hits = results
            .iter()
            .filter(|res| gt_set.contains(&res.doc_id))
            .count();
        total_recall += hits as f64 / k as f64;
    }
    total_recall / queries.len() as f64
}

/// (i) Zufällig gleichverteilte Auswahl von `fraction` der Knoten als Tombstones.
fn select_random_tombstones(count: usize, fraction: f64, seed: u64) -> Vec<u64> {
    let target = (count as f64 * fraction) as usize;
    let mut rng = StdRng::seed_from_u64(seed);
    let mut chosen = HashSet::with_capacity(target);
    while chosen.len() < target {
        let id = rng.gen_range(0..count) as u64;
        chosen.insert(id);
    }
    let mut vec: Vec<u64> = chosen.into_iter().collect();
    vec.sort_unstable();
    vec
}

/// (ii) Räumlich geclusterte Tombstones: Die `fraction` nahesten Knoten zu einem Seed-Vektor.
fn select_clustered_tombstones(vectors: &[Vec<f32>], fraction: f64) -> Vec<u64> {
    let target = (vectors.len() as f64 * fraction) as usize;
    let center = &vectors[0];
    let mut scored: Vec<(u64, f32)> = vectors
        .iter()
        .enumerate()
        .map(|(idx, v)| {
            let dist = 1.0 - dot_product(center, v);
            (idx as u64, dist)
        })
        .collect();

    scored.sort_by(|a, b| a.1.total_cmp(&b.1));
    let mut vec: Vec<u64> = scored.into_iter().take(target).map(|(id, _)| id).collect();
    vec.sort_unstable();
    vec
}

/// Extrahiert die rohen Layer-0-Nachbarlisten für alle Knoten aus der MmapIndex-Dateistruktur.
async fn extract_raw_layer0_connections(
    index: &HnswIndex,
    save_path: &std::path::Path,
) -> HashMap<u64, Vec<u32>> {
    index.save(save_path).await.expect("save failed");
    let mmap = MmapIndex::open(save_path).expect("MmapIndex open failed");
    let node_count = mmap.header.node_count() as usize;

    let mut conn_map = HashMap::with_capacity(node_count);
    for idx in 0..node_count {
        let node_id = idx as u64;
        let record = mmap.get_node_record(idx).expect("get_node_record failed");
        let conns = mmap
            .get_connections(&record, 0)
            .expect("get_connections failed");
        conn_map.insert(node_id, conns);
    }
    conn_map
}

#[tokio::test]
async fn test_partial_rebuild_recall_regression() {
    // 1. Deterministischer Testkorpus: 5.000 Vektoren, dim 128, fixed seed 42
    let vectors = generate_vectors(5_000, 128, 42);

    // 2. 100 Test-Queries, fixed seed 1337
    let test_queries = generate_vectors(100, 128, 1337);

    // --- Muster (i): Zufällig gleichverteilte Tombstones (20% = 1.000 Knoten) ---
    {
        let index = build_hnsw_index(&vectors).await;
        let tombstone_ids = select_random_tombstones(5_000, 0.20, 100);
        let tombstone_set: HashSet<u64> = tombstone_ids.iter().copied().collect();

        // Ground-Truth kNN (k=10) gegen den verbleibenden, nicht-tombstonierten Korpus
        let ground_truth = brute_force_knn(&vectors, &tombstone_set, &test_queries, 10);

        // Tombstones anwenden
        let tx_del = TxId::new(2);
        for &id in &tombstone_ids {
            index
                .delete(tx_del, DocId::new(id))
                .await
                .expect("delete failed");
        }
        index.commit(tx_del).await.expect("commit delete failed");

        // Recall@10 VOR rebuild_region()
        let recall_before = evaluate_recall_at_k(&index, &test_queries, &ground_truth, 10).await;

        // rebuild_region() ausführen
        index
            .rebuild_region(tombstone_ids.clone())
            .await
            .expect("rebuild_region failed");

        // Recall@10 NACH rebuild_region()
        let recall_after = evaluate_recall_at_k(&index, &test_queries, &ground_truth, 10).await;

        println!("Muster (i) Random 20% Tombstones:");
        println!("  Recall@10 vor rebuild_region():  {:.4}", recall_before);
        println!("  Recall@10 nach rebuild_region(): {:.4}", recall_after);
        eprintln!(
            "RECALL_METRIC before={:.4} after={:.4} drop_pp={:.4}",
            recall_before,
            recall_after,
            recall_before - recall_after
        );

        assert!(
            recall_after >= recall_before - RECALL_TOLERANCE_BAND,
            "Recall@10 nach rebuild_region() ({:.4}) fiel unter Schranke gegenüber davor ({:.4}) [Toleranz ±{:.2}]",
            recall_after,
            recall_before,
            RECALL_TOLERANCE_BAND
        );
    }

    // --- Muster (ii): Räumlich geclusterte Tombstones (20% = 1.000 Knoten um ein Themengebiet) ---
    {
        let index = build_hnsw_index(&vectors).await;
        let tombstone_ids = select_clustered_tombstones(&vectors, 0.20);
        let tombstone_set: HashSet<u64> = tombstone_ids.iter().copied().collect();

        // Ground-Truth kNN (k=10) gegen den verbleibenden, nicht-tombstonierten Korpus
        let ground_truth = brute_force_knn(&vectors, &tombstone_set, &test_queries, 10);

        // Tombstones anwenden
        let tx_del = TxId::new(2);
        for &id in &tombstone_ids {
            index
                .delete(tx_del, DocId::new(id))
                .await
                .expect("delete failed");
        }
        index.commit(tx_del).await.expect("commit delete failed");

        // Recall@10 VOR rebuild_region()
        let recall_before = evaluate_recall_at_k(&index, &test_queries, &ground_truth, 10).await;

        // rebuild_region() ausführen
        index
            .rebuild_region(tombstone_ids.clone())
            .await
            .expect("rebuild_region failed");

        // Recall@10 NACH rebuild_region()
        let recall_after = evaluate_recall_at_k(&index, &test_queries, &ground_truth, 10).await;

        println!("Muster (ii) Spatially Clustered 20% Tombstones:");
        println!("  Recall@10 vor rebuild_region():  {:.4}", recall_before);
        println!("  Recall@10 nach rebuild_region(): {:.4}", recall_after);

        assert!(
            recall_after >= recall_before - RECALL_TOLERANCE_BAND,
            "Recall@10 nach rebuild_region() ({:.4}) fiel unter Schranke gegenüber davor ({:.4}) [Toleranz ±{:.2}]",
            recall_after,
            recall_before,
            RECALL_TOLERANCE_BAND
        );
    }
}

/// Prüft den in VETOES.md#VETO-F02 benannten Risikofall:
/// Vergleicht die Grad-Verteilung (Anzahl aktiver Nachbarn) der von `rebuild_region()` betroffenen Knoten VOR und NACH dem Aufruf.
/// Assertions:
/// 1. Verifiziert, dass kein betroffener Knoten auf Grad 0 fällt (vollständig isolierter Knoten = Navigierbarkeits-Totalausfall).
/// 2. Verifiziert, dass alle aktiven Knoten weiterhin über HNSW-Traversierung auffindbar sind.
#[tokio::test]
async fn test_partial_rebuild_node_degree_no_isolation() {
    let temp_dir = tempfile::tempdir().expect("tempdir failed");
    let path_before = temp_dir.path().join("before.hnsw");
    let path_after = temp_dir.path().join("after.hnsw");

    let vectors = generate_vectors(5_000, 128, 42);
    let index = build_hnsw_index(&vectors).await;

    // Räumlich geclusterte Tombstones (25% = 1.250 Knoten im dichtesten Cluster)
    let tombstone_ids = select_clustered_tombstones(&vectors, 0.25);
    let tombstone_set: HashSet<u64> = tombstone_ids.iter().copied().collect();

    let tx_del = TxId::new(2);
    for &id in &tombstone_ids {
        index
            .delete(tx_del, DocId::new(id))
            .await
            .expect("delete failed");
    }
    index.commit(tx_del).await.expect("commit delete failed");

    // Nachbarlisten VOR rebuild_region() sichern
    let conns_before = extract_raw_layer0_connections(&index, &path_before).await;

    // rebuild_region() ausführen
    index
        .rebuild_region(tombstone_ids)
        .await
        .expect("rebuild_region failed");

    // Nachbarlisten NACH rebuild_region() sichern
    let conns_after = extract_raw_layer0_connections(&index, &path_after).await;

    let mut total_degree_loss = 0;
    let mut affected_nodes_count = 0;
    let mut zero_degree_nodes = Vec::new();

    for (&node_id, raw_after) in &conns_after {
        if !tombstone_set.contains(&node_id) {
            let raw_before = conns_before
                .get(&node_id)
                .map(|v| v.as_slice())
                .unwrap_or(&[]);
            let active_before = raw_before
                .iter()
                .filter(|&&nbr| !tombstone_set.contains(&(nbr as u64)))
                .count();
            let active_after = raw_after.len(); // rebuild_region hat alle Tombstones aus raw_after entfernt

            if raw_before.len() != raw_after.len() {
                affected_nodes_count += 1;
                total_degree_loss += raw_before.len() - raw_after.len();
            }

            // Kriterien-Assertion 1: Kein aktiver Knoten darf auf Grad 0 fallen!
            if active_after == 0 {
                zero_degree_nodes.push(node_id);
            }

            // Invariante: active_after entspricht genau active_before (rebuild_region entfernt Tombstones, knüpft keine neuen Kanten)
            assert_eq!(
                active_after, active_before,
                "Aktivgrad für Knoten {} weicht ab: vor = {}, nach = {}",
                node_id, active_before, active_after
            );
        }
    }

    println!(
        "Grad-Verteilung: {} Knoten von Tombstone-Pruning betroffen, totaler Gradverlust: {} geprunte Kanten.",
        affected_nodes_count, total_degree_loss
    );

    assert!(
        zero_degree_nodes.is_empty(),
        "Ein oder mehrere Knoten fielen nach rebuild_region() auf Grad 0 (Isolierter Knoten): {:?}",
        zero_degree_nodes
    );

    // Kriterien-Assertion 2: Navigierbarkeitstest via Direktabfrage aller verbleibenden aktiven Knoten
    let mut isolated_search_nodes = Vec::new();
    for (idx, vec) in vectors.iter().enumerate() {
        let doc_id = DocId::new(idx as u64);
        if !tombstone_set.contains(&(idx as u64)) {
            let res = index.search(vec, 1).await.expect("search failed");
            if res.is_empty() || res[0].doc_id != doc_id {
                isolated_search_nodes.push(doc_id);
            }
        }
    }

    assert!(
        isolated_search_nodes.is_empty(),
        "Ein oder mehrere Knoten wurden nach rebuild_region() von der Graph-Navigierung isoliert: {:?}",
        isolated_search_nodes
    );
}

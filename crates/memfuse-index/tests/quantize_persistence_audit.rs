// FILE-CONTEXT
// ZWECK: Audit-Testsuite für SQ8 Kendall-Tau Rangkorrelation, Persistence Roundtrips und Mmap Fault-Tolerance
// STAND: TS:2026-08-31T00:00:00Z

use memfuse_core::traits::VectorIndex;
use memfuse_core::types::{DistanceMetric, DocId, TxId};
use memfuse_core::MemFuseError;
use memfuse_index::diskann::{DiskAnnConfig, DiskAnnIndex};
use memfuse_index::hnsw::{HnswConfig, HnswIndex};
use memfuse_index::persistence::MmapIndex;
use memfuse_index::quantize::ScalarQuantizer;
use rand::Rng;

/// Independently computes Kendall-Tau rank correlation between two distance rankings.
/// Tau = (C - D) / (0.5 * n * (n - 1))
fn kendall_tau_correlation(rank_f32: &[DocId], rank_sq8: &[DocId]) -> f64 {
    let n = rank_f32.len();
    if n <= 1 {
        return 1.0;
    }

    // Map doc_id to rank position in rank_f32
    let pos_f32: std::collections::HashMap<DocId, usize> = rank_f32
        .iter()
        .enumerate()
        .map(|(rank, &id)| (id, rank))
        .collect();

    // Get rank positions in rank_f32 for items as ordered by rank_sq8
    let sq8_positions: Vec<usize> = rank_sq8
        .iter()
        .filter_map(|id| pos_f32.get(id).copied())
        .collect();

    let m = sq8_positions.len();
    if m <= 1 {
        return 0.0;
    }

    let mut concordant = 0;
    let mut discordant = 0;

    for i in 0..m {
        for j in (i + 1)..m {
            if sq8_positions[i] < sq8_positions[j] {
                concordant += 1;
            } else if sq8_positions[i] > sq8_positions[j] {
                discordant += 1;
            }
        }
    }

    let total_pairs = (m * (m - 1)) / 2;
    if total_pairs == 0 {
        return 1.0;
    }

    (concordant as f64 - discordant as f64) / (total_pairs as f64)
}

#[test]
fn test_sq8_kendall_tau_rank_correlation() {
    let dim = 128;
    let num_vectors = 200;
    let mut rng = rand::thread_rng();

    // 1. Uniform Distribution Data
    let mut uniform_vectors = Vec::with_capacity(num_vectors);
    for _ in 0..num_vectors {
        let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        uniform_vectors.push(v);
    }
    let uniform_refs: Vec<&[f32]> = uniform_vectors.iter().map(|v| v.as_slice()).collect();
    let quantizer_uniform = ScalarQuantizer::train(&uniform_refs, dim);
    let uniform_quantized: Vec<Vec<u8>> = uniform_vectors
        .iter()
        .map(|v| quantizer_uniform.quantize(v))
        .collect();

    // 2. Skewed / Heterogeneous Distribution Data (Dimensions 0..10 are 100x larger)
    let mut skewed_vectors = Vec::with_capacity(num_vectors);
    for _ in 0..num_vectors {
        let mut v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        for i in 0..10 {
            v[i] *= 100.0;
        }
        skewed_vectors.push(v);
    }
    let skewed_refs: Vec<&[f32]> = skewed_vectors.iter().map(|v| v.as_slice()).collect();
    let quantizer_skewed = ScalarQuantizer::train(&skewed_refs, dim);
    let skewed_quantized: Vec<Vec<u8>> = skewed_vectors
        .iter()
        .map(|v| quantizer_skewed.quantize(v))
        .collect();

    // Evaluate Kendall-Tau across 30 query vectors
    let num_queries = 30;
    let k = 20;

    let mut tau_uniform_sum = 0.0f64;
    let mut tau_skewed_sum = 0.0f64;

    for _ in 0..num_queries {
        // Query for Uniform
        let q_uniform: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();

        let mut f32_dists_u: Vec<(DocId, f32)> = uniform_vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let d = memfuse_index::distance::cosine_distance_scalar(&q_uniform, v);
                (DocId::new((idx + 1) as u64), d)
            })
            .collect();
        f32_dists_u.sort_by(|a, b| a.1.total_cmp(&b.1));
        let rank_f32_u: Vec<DocId> = f32_dists_u.into_iter().take(k).map(|(id, _)| id).collect();

        let mut sq8_dists_u: Vec<(DocId, f32)> = uniform_quantized
            .iter()
            .enumerate()
            .map(|(idx, qv)| {
                let d = quantizer_uniform
                    .asymmetric_dist(&q_uniform, qv, DistanceMetric::Cosine)
                    .unwrap();
                (DocId::new((idx + 1) as u64), d)
            })
            .collect();
        sq8_dists_u.sort_by(|a, b| a.1.total_cmp(&b.1));
        let rank_sq8_u: Vec<DocId> = sq8_dists_u.into_iter().take(k).map(|(id, _)| id).collect();

        tau_uniform_sum += kendall_tau_correlation(&rank_f32_u, &rank_sq8_u);

        // Query for Skewed
        let mut q_skewed: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        for i in 0..10 {
            q_skewed[i] *= 100.0;
        }

        let mut f32_dists_s: Vec<(DocId, f32)> = skewed_vectors
            .iter()
            .enumerate()
            .map(|(idx, v)| {
                let d = memfuse_index::distance::cosine_distance_scalar(&q_skewed, v);
                (DocId::new((idx + 1) as u64), d)
            })
            .collect();
        f32_dists_s.sort_by(|a, b| a.1.total_cmp(&b.1));
        let rank_f32_s: Vec<DocId> = f32_dists_s.into_iter().take(k).map(|(id, _)| id).collect();

        let mut sq8_dists_s: Vec<(DocId, f32)> = skewed_quantized
            .iter()
            .enumerate()
            .map(|(idx, qv)| {
                let d = quantizer_skewed
                    .asymmetric_dist(&q_skewed, qv, DistanceMetric::Cosine)
                    .unwrap();
                (DocId::new((idx + 1) as u64), d)
            })
            .collect();
        sq8_dists_s.sort_by(|a, b| a.1.total_cmp(&b.1));
        let rank_sq8_s: Vec<DocId> = sq8_dists_s.into_iter().take(k).map(|(id, _)| id).collect();

        tau_skewed_sum += kendall_tau_correlation(&rank_f32_s, &rank_sq8_s);
    }

    let avg_tau_uniform = tau_uniform_sum / num_queries as f64;
    let avg_tau_skewed = tau_skewed_sum / num_queries as f64;

    println!("\n=== SQ8 KENDALL-TAU RANK CORRELATION ===");
    println!("Uniform Distribution Tau: {:.4}", avg_tau_uniform);
    println!("Skewed Distribution Tau: {:.4}", avg_tau_skewed);

    assert!(
        avg_tau_uniform > 0.80,
        "Uniform SQ8 Kendall-Tau correlation ({avg_tau_uniform:.4}) below 0.80"
    );
    assert!(
        avg_tau_skewed > 0.80,
        "Skewed SQ8 Kendall-Tau correlation ({avg_tau_skewed:.4}) below 0.80"
    );
}

#[tokio::test]
async fn test_hnsw_and_diskann_persistence_roundtrip() {
    let temp_dir = tempfile::tempdir().unwrap();
    let hnsw_path = temp_dir.path().join("roundtrip.hnsw");
    let diskann_path = temp_dir.path().join("roundtrip.idx");

    let dim = 32;
    let n = 100;
    let mut rng = rand::thread_rng();

    let mut vectors = Vec::with_capacity(n);
    let mut ids = Vec::with_capacity(n);
    for i in 0..n {
        let v: Vec<f32> = (0..dim).map(|_| rng.gen_range(-1.0..1.0)).collect();
        vectors.push(v);
        ids.push(DocId::new((i + 1) as u64));
    }

    // 1. HNSW Persistence Roundtrip
    let hnsw_config = HnswConfig {
        dimension: dim,
        m: 16,
        ef_construction: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let hnsw_index = HnswIndex::try_new(hnsw_config.clone()).unwrap();
    let tx = TxId::new(1);
    for (id, v) in ids.iter().zip(vectors.iter()) {
        hnsw_index.insert(tx, *id, v).await.unwrap();
    }
    hnsw_index.commit(tx).await.unwrap();

    let query = &vectors[25];
    let hnsw_pre_results = hnsw_index.search(query, 5).await.unwrap();

    // Save and Reload
    hnsw_index.save(&hnsw_path).await.unwrap();
    let hnsw_reloaded = HnswIndex::try_new(hnsw_config).unwrap();
    hnsw_reloaded.load_mmap(&hnsw_path).await.unwrap();

    let hnsw_post_results = hnsw_reloaded.search(query, 5).await.unwrap();

    assert_eq!(
        hnsw_pre_results.len(),
        hnsw_post_results.len(),
        "HNSW search result counts before and after save/reload must match"
    );
    for (pre, post) in hnsw_pre_results.iter().zip(hnsw_post_results.iter()) {
        assert_eq!(pre.doc_id, post.doc_id, "HNSW DocId mismatch post reload");
        assert!(
            (pre.score - post.score).abs() < 1e-5,
            "HNSW score mismatch post reload: pre={}, post={}",
            pre.score,
            post.score
        );
    }

    // Direct Mmap Index Header Verification
    let mmap_hnsw = MmapIndex::open(&hnsw_path).unwrap();
    assert_eq!(mmap_hnsw.header.node_count as usize, n);
    assert_eq!(mmap_hnsw.header.dimension as usize, dim);

    // 2. DiskANN Persistence Roundtrip
    let diskann_config = DiskAnnConfig {
        index_path: diskann_path.clone(),
        dimension: dim,
        max_degree: 16,
        beam_width: 16,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };
    let diskann_index = DiskAnnIndex::try_new(diskann_config.clone()).unwrap();
    diskann_index.build(&vectors, &ids).await.unwrap();

    let diskann_pre_results = diskann_index.search(query, 5).await.unwrap();

    let diskann_reloaded = DiskAnnIndex::try_new(diskann_config).unwrap();
    diskann_reloaded.load().await.unwrap();

    let diskann_post_results = diskann_reloaded.search(query, 5).await.unwrap();

    assert_eq!(diskann_pre_results.len(), diskann_post_results.len());
    for (pre, post) in diskann_pre_results.iter().zip(diskann_post_results.iter()) {
        assert_eq!(
            pre.doc_id, post.doc_id,
            "DiskANN DocId mismatch post reload"
        );
        assert!(
            (pre.score - post.score).abs() < 1e-5,
            "DiskANN score mismatch post reload"
        );
    }
}

#[tokio::test]
async fn test_corrupted_mmap_file_handling() {
    let temp_dir = tempfile::tempdir().unwrap();

    // 1. Completely Truncated / Empty File
    let empty_path = temp_dir.path().join("empty.hnsw");
    tokio::fs::write(&empty_path, b"").await.unwrap();
    let res_empty_hnsw = MmapIndex::open(&empty_path);
    assert!(
        matches!(res_empty_hnsw, Err(MemFuseError::Storage(_))),
        "Opening empty file must return MemFuseError::Storage"
    );

    // 2. Corrupt Magic Bytes in HNSW file
    let bad_magic_path = temp_dir.path().join("bad_magic.hnsw");
    let mut bad_bytes = vec![0u8; 64];
    bad_bytes[0..4].copy_from_slice(b"BADM");
    tokio::fs::write(&bad_magic_path, &bad_bytes).await.unwrap();
    let res_bad_magic = MmapIndex::open(&bad_magic_path);
    assert!(matches!(res_bad_magic, Err(MemFuseError::Storage(_))));

    // 3. DiskANN Corrupt Magic Bytes
    let bad_diskann_path = temp_dir.path().join("bad_magic.idx");
    tokio::fs::write(&bad_diskann_path, &bad_bytes)
        .await
        .unwrap();
    let diskann = DiskAnnIndex::try_new(DiskAnnConfig {
        index_path: bad_diskann_path,
        ..Default::default()
    })
    .unwrap();
    let res_diskann_load = diskann.load().await;
    assert!(matches!(res_diskann_load, Err(MemFuseError::Storage(_))));

    // 4. DiskANN Truncated Body (Valid header, but missing vector data)
    let trunc_diskann_path = temp_dir.path().join("truncated_body.idx");
    let header_bytes = [
        b'D', b'A', b'N', b'N', // magic
        1u8, 0u8, // version 1
        100u8, 0, 0, 0, 0, 0, 0, 0, // node_count = 100
        128u8, 0, 0, 0, // dimension = 128
        64u8, 0, 0, 0, // max_degree = 64
        0u8, 16u8, 0, 0, // sector_size = 4096
        0u8, 0, 0, 0, // entry_point = 0
        0u8, 0u8, // metric, quantized
        0u8, 0, 0, 0, // q_min
        0u8, 0, 0, 0, // q_max
    ];
    let mut trunc_data = Vec::from(header_bytes);
    trunc_data.resize(4096, 0u8); // Pad header sector
    tokio::fs::write(&trunc_diskann_path, &trunc_data)
        .await
        .unwrap();

    let trunc_diskann = DiskAnnIndex::try_new(DiskAnnConfig {
        index_path: trunc_diskann_path,
        dimension: 128,
        sector_size: 4096,
        ..Default::default()
    })
    .unwrap();

    let load_res = trunc_diskann.load().await;
    assert!(
        load_res.is_err(),
        "Loading truncated mmap file must return Err(MemFuseError) without panic"
    );
}

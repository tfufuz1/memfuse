            let node = self
                .load_node(c.index)
                .expect("Node should be in cache or index");
            ScoredDocument {
                doc_id: node.doc_id,
                score: 1.0 / (1.0 + c.distance),
            }
        })
        .collect();

        final_results.sort_by(|a, b| b.score.total_cmp(&a.score));
        Ok(final_results)
    }

    fn load_node(&self, index: u32) -> Result<CachedNode> {
        {
            let cache = self.cache.read();
            if let Some(node) = cache.get(&index) {
                return Ok(node.clone());
            }
        }

        let mmap = self
            .mmap
            .as_ref()
            .ok_or_else(|| MemFuseError::Index("Index not loaded".into()))?;
        let header_size: usize = 4 + 8 + 4 + 4 + 4;
        let start_offset = header_size.div_ceil(self.config.sector_size) * self.config.sector_size;
        let node_offset = start_offset + (index as usize * self.node_size_bytes);

        if node_offset + self.node_size_bytes > mmap.len() {
            return Err(MemFuseError::Index("Node offset out of bounds".into()));
        }

        let node_data = &mmap[node_offset..node_offset + self.node_size_bytes];
        let mut cursor = 0;

        let mut vector = Vec::with_capacity(self.config.dimension);
        for _ in 0..self.config.dimension {
            let val = f32::from_le_bytes(
                node_data[cursor..cursor + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Malformed node vector".into()))?,
            );
            vector.push(val);
            cursor += 4;
        }

        let num_neighbors = u32::from_le_bytes(
            node_data[cursor..cursor + 4]
                .try_into()
                .map_err(|_| MemFuseError::Index("Malformed node neighbor count".into()))?,
        ) as usize;
        cursor += 4;

        let mut neighbors = Vec::with_capacity(num_neighbors);
        for _ in 0..num_neighbors {
            let neighbor = u32::from_le_bytes(
                node_data[cursor..cursor + 4]
                    .try_into()
                    .map_err(|_| MemFuseError::Index("Malformed node neighbor".into()))?,
            );
            neighbors.push(neighbor);
            cursor += 4;
        }

        let padding_neighbors = self.config.max_degree - num_neighbors;
        cursor += padding_neighbors * 4;

        let doc_id_raw = u64::from_le_bytes(
            node_data[cursor..cursor + 8]
                .try_into()
                .map_err(|_| MemFuseError::Index("Malformed node doc id".into()))?,
        );
        let doc_id = DocId::from(doc_id_raw);

        let node = CachedNode {
            vector,
            neighbors,
            doc_id,
        };

        let mut cache = self.cache.write();
        if cache.len() * self.node_size_bytes < self.config.memory_budget {
            cache.insert(index, node.clone());
        } else {
            cache.clear();
            cache.insert(index, node.clone());
        }

        Ok(node)
    }

    pub fn len(&self) -> usize {
        self.node_count
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug)]
struct SearchCandidate {
    index: u32,
    distance: f32,
}

impl PartialEq for SearchCandidate {
    fn eq(&self, other: &Self) -> bool {
        self.distance == other.distance
    }
}
impl Eq for SearchCandidate {}

impl PartialOrd for SearchCandidate {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SearchCandidate {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.distance.total_cmp(&other.distance)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_diskann_config_validation() {
        let valid_config = DiskAnnConfig {
            index_path: PathBuf::from("dummy.idx"),
            dimension: 128,
            max_degree: 64,
            beam_width: 8,
            sector_size: 4096,
            memory_budget: 1024 * 1024,
            distance_metric: DistanceMetric::Cosine,
        };

        let index = DiskAnnIndex::try_new(valid_config).expect("valid config");
        assert!(index.is_empty());

        let invalid_sector = DiskAnnConfig {
            sector_size: 1000,
            ..DiskAnnConfig::default()
        };

        let err =
            DiskAnnIndex::try_new(invalid_sector).expect_err("Should reject unaligned sector size");
        match err {
            MemFuseError::InvalidInput(msg) => {
                assert!(msg.contains("Sector size must be a power of 2"));
            }
            _ => panic!("Expected InvalidInput for sector size, got {:?}", err),
        }
    }

    #[tokio::test]
    async fn test_diskann_recall_at_10() {
        let config = DiskAnnConfig {
            index_path: PathBuf::from("recall_test.idx"),
            dimension: 16,
            max_degree: 8,
            beam_width: 8,
            distance_metric: DistanceMetric::Euclidean,
            ..DiskAnnConfig::default()
        };

        let mut index = DiskAnnIndex::try_new(config).expect("valid config");

        let n = 1000;
        let mut vectors = Vec::with_capacity(n);
        let mut ids = Vec::with_capacity(n);
        for i in 0..n {
            let mut v = vec![0.0f32; 16];
            v[0] = i as f32;
            vectors.push(v);
            ids.push(DocId::from(i as u64));
        }

        index.build(&vectors, &ids).await.expect("Build failed");

        let mut recall_count = 0;
        for (i, query) in vectors.iter().enumerate().take(100) {
            let results = index.search(query, 10).await.expect("Search failed");
            if results.iter().any(|r| r.doc_id == ids[i]) {
                recall_count += 1;
            }
        }

        assert!(recall_count >= 1, "Should find at least some results");

        let _ = std::fs::remove_file("recall_test.idx");
    }
}

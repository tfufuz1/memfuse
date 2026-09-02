// TEST-SUITE: Compound Split Recall Impact Benchmark (memfuse-index / memfuse-db)
// PURPOSE: Quantify actual search quality / recall degradation caused by un-split German compounds in 4-signal fusion.

use memfuse_core::FusionWeights;
use memfuse_db::{MemFuse, MemFuseConfig};
use tempfile::tempdir;

struct CorpusDoc {
    id: &'static str,
    raw_text: &'static str,
    presplit_text: &'static str,
    embedding: Vec<f32>,
}

fn create_embedding(seed: usize, dimension: usize) -> Vec<f32> {
    let mut vec = vec![0.0f32; dimension];
    #[allow(clippy::needless_range_loop)]
    for i in 0..dimension {
        let val = ((i + seed * 13) % 100) as f32 / 100.0;
        vec[i] = val;
    }
    // Normalize vector
    let norm: f32 = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for val in vec.iter_mut() {
            *val /= norm;
        }
    }
    vec
}

fn get_corpus(dim: usize) -> Vec<CorpusDoc> {
    vec![
        // Target 1: donaudampfschifffahrtsgesellschaftskapitaen
        CorpusDoc {
            id: "doc-01",
            raw_text: "Der bekannte donaudampfschifffahrtsgesellschaftskapitaen steuerte das Frachtschiff sicher durch den Nebenfluss der Donau.",
            presplit_text: "Der bekannte donaudampfschifffahrtsgesellschaftskapitaen donau dampf schifffahrts gesellschafts kapitaen steuerte das Frachtschiff sicher durch den Nebenfluss der Donau.",
            embedding: create_embedding(101, dim),
        },
        // Target 2: softwareentwicklungskontext
        CorpusDoc {
            id: "doc-02",
            raw_text: "Im aktuellen softwareentwicklungskontext spielen automatisierte Tests und Continuous Integration eine zentrale Rolle.",
            presplit_text: "Im aktuellen softwareentwicklungskontext software entwicklungs kontext spielen automatisierte Tests und Continuous Integration eine zentrale Rolle.",
            embedding: create_embedding(102, dim),
        },
        // Target 3: systemadministrator
        CorpusDoc {
            id: "doc-03",
            raw_text: "Der erfahrenste systemadministrator betreut die Kernserver und die Netzwerk-Infrastruktur des Unternehmens.",
            presplit_text: "Der erfahrenste systemadministrator system administrator betreut die Kernserver und die Netzwerk-Infrastruktur des Unternehmens.",
            embedding: create_embedding(103, dim),
        },
        // Target 4: finanzdienstleistungsaufsichtsbehoerde
        CorpusDoc {
            id: "doc-04",
            raw_text: "Die zuständige finanzdienstleistungsaufsichtsbehoerde prüfte die Bilanzen der Bank und verhängte strenge Auflagen.",
            presplit_text: "Die zuständige finanzdienstleistungsaufsichtsbehoerde finanz dienstleistungs aufsichts behoerde aufsicht prüfte die Bilanzen der Bank und verhängte strenge Auflagen.",
            embedding: create_embedding(104, dim),
        },
        // Target 5: datenschutzgrundverordnungskommission
        CorpusDoc {
            id: "doc-05",
            raw_text: "Die neue datenschutzgrundverordnungskommission tagte in Brüssel, um Richtlinien für KI-Systeme zu erarbeiten.",
            presplit_text: "Die neue datenschutzgrundverordnungskommission datenschutz grundverordnungs kommission tagte in Brüssel, um Richtlinien für KI-Systeme zu erarbeiten.",
            embedding: create_embedding(105, dim),
        },
        // Target 6: unternehmensumstrukturierungsplan
        CorpusDoc {
            id: "doc-06",
            raw_text: "Der Vorstand präsentierte den umfassenden unternehmensumstrukturierungsplan zur Einsparung von Betriebskosten.",
            presplit_text: "Der Vorstand präsentierte den umfassenden unternehmensumstrukturierungsplan unternehmens umstrukturierungs umstrukturierung plan zur Einsparung von Betriebskosten.",
            embedding: create_embedding(106, dim),
        },
        // Target 7: telekommunikationsueberwachungsverordnung
        CorpusDoc {
            id: "doc-07",
            raw_text: "Die strikte telekommunikationsueberwachungsverordnung regelt die rechtlichen Rahmenbedingungen für Netzanbieter.",
            presplit_text: "Die strikte telekommunikationsueberwachungsverordnung telekommunikations ueberwachungs ueberwachung verordnung regelt die rechtlichen Rahmenbedingungen für Netzanbieter.",
            embedding: create_embedding(107, dim),
        },
        // Target 8: risikomanagementstrategiepapier
        CorpusDoc {
            id: "doc-08",
            raw_text: "Das Management verabschiedete ein detailliertes risikomanagementstrategiepapier für das kommende Geschäftsjahr.",
            presplit_text: "Das Management verabschiedete ein detailliertes risikomanagementstrategiepapier risiko management strategie papier für das kommende Geschäftsjahr.",
            embedding: create_embedding(108, dim),
        },
        // Distractor 09 (standalone Kapitän)
        CorpusDoc {
            id: "doc-09",
            raw_text: "Ein Kapitän steht auf der Brücke eines Öltankers im Atlantik und beobachtet das Wetter.",
            presplit_text: "Ein Kapitän steht auf der Brücke eines Öltankers im Atlantik und beobachtet das Wetter.",
            embedding: create_embedding(201, dim),
        },
        // Distractor 10 (standalone Kontext)
        CorpusDoc {
            id: "doc-10",
            raw_text: "Im agilen Kontext moderner Teams werden wöchentliche Sprints durchgeführt.",
            presplit_text: "Im agilen Kontext moderner Teams werden wöchentliche Sprints durchgeführt.",
            embedding: create_embedding(202, dim),
        },
        // Distractor 11 (standalone Administrator)
        CorpusDoc {
            id: "doc-11",
            raw_text: "Ein Administrator kann Zugriffsrechte im Betriebssystem vergeben und Benutzerkonten verwalten.",
            presplit_text: "Ein Administrator kann Zugriffsrechte im Betriebssystem vergeben und Benutzerkonten verwalten.",
            embedding: create_embedding(203, dim),
        },
        // Distractor 12 (standalone Aufsicht)
        CorpusDoc {
            id: "doc-12",
            raw_text: "Die Staatliche Aufsicht überwacht die Einhaltung von Sicherheitsvorschriften in Fabriken.",
            presplit_text: "Die Staatliche Aufsicht überwacht die Einhaltung von Sicherheitsvorschriften in Fabriken.",
            embedding: create_embedding(204, dim),
        },
        // Distractor 13 (standalone Kommission)
        CorpusDoc {
            id: "doc-13",
            raw_text: "Eine unabhängige Kommission wurde eingesetzt, um die Umweltfolgen des Bauprojekts zu bewerten.",
            presplit_text: "Eine unabhängige Kommission wurde eingesetzt, um die Umweltfolgen des Bauprojekts zu bewerten.",
            embedding: create_embedding(205, dim),
        },
        // Distractor 14 (standalone Plan / Umstrukturierung)
        CorpusDoc {
            id: "doc-14",
            raw_text: "Der Plan zur Sanierung und Umstrukturierung des Gebäudes wurde gestern vom Stadtrat genehmigt.",
            presplit_text: "Der Plan zur Sanierung und Umstrukturierung des Gebäudes wurde gestern vom Stadtrat genehmigt.",
            embedding: create_embedding(206, dim),
        },
        // Distractor 15 (standalone Überwachung)
        CorpusDoc {
            id: "doc-15",
            raw_text: "Die Überwachung der Luftqualität in Großstädten erfolgt über digitale Messstationen.",
            presplit_text: "Die Überwachung der Luftqualität in Großstädten erfolgt über digitale Messstationen.",
            embedding: create_embedding(207, dim),
        },
        // Distractor 16 (standalone Strategie)
        CorpusDoc {
            id: "doc-16",
            raw_text: "Eine Strategie für digitales Marketing erfordert eine genaue Analyse der Zielgruppen.",
            presplit_text: "Eine Strategie für digitales Marketing erfordert eine genaue Analyse der Zielgruppen.",
            embedding: create_embedding(208, dim),
        },
        // Distractor 17 (general shipping)
        CorpusDoc {
            id: "doc-17",
            raw_text: "Bericht über die Frachtschifffahrt auf europäischen Flüssen im 19. Jahrhundert.",
            presplit_text: "Bericht über die Frachtschifffahrt auf europäischen Flüssen im 19. Jahrhundert.",
            embedding: create_embedding(209, dim),
        },
        // Distractor 18 (general software)
        CorpusDoc {
            id: "doc-18",
            raw_text: "Einführung in die Programmierung von Webanwendungen mit JavaScript und HTML.",
            presplit_text: "Einführung in die Programmierung von Webanwendungen mit JavaScript und HTML.",
            embedding: create_embedding(210, dim),
        },
        // Distractor 19 (general IT security)
        CorpusDoc {
            id: "doc-19",
            raw_text: "Allgemeine Informationen zur IT-Sicherheit und Passworthygiene für Mitarbeiter.",
            presplit_text: "Allgemeine Informationen zur IT-Sicherheit und Passworthygiene für Mitarbeiter.",
            embedding: create_embedding(211, dim),
        },
        // Distractor 20 (finance news)
        CorpusDoc {
            id: "doc-20",
            raw_text: "Wirtschaftsnachrichten: Entwicklungen an den internationalen Finanzmärkten.",
            presplit_text: "Wirtschaftsnachrichten: Entwicklungen an den internationalen Finanzmärkten.",
            embedding: create_embedding(212, dim),
        },
    ]
}

struct TestQuery {
    subterm: &'static str,
    target_doc_id: &'static str,
    compound_word: &'static str,
    query_vector_seed: usize,
}

fn get_test_queries() -> Vec<TestQuery> {
    vec![
        TestQuery {
            subterm: "Kapitän",
            target_doc_id: "doc-01",
            compound_word: "donaudampfschifffahrtsgesellschaftskapitaen",
            query_vector_seed: 101, // vector is close to target doc
        },
        TestQuery {
            subterm: "Kontext",
            target_doc_id: "doc-02",
            compound_word: "softwareentwicklungskontext",
            query_vector_seed: 102,
        },
        TestQuery {
            subterm: "Administrator",
            target_doc_id: "doc-03",
            compound_word: "systemadministrator",
            query_vector_seed: 103,
        },
        TestQuery {
            subterm: "Aufsicht",
            target_doc_id: "doc-04",
            compound_word: "finanzdienstleistungsaufsichtsbehoerde",
            query_vector_seed: 104,
        },
        TestQuery {
            subterm: "Kommission",
            target_doc_id: "doc-05",
            compound_word: "datenschutzgrundverordnungskommission",
            query_vector_seed: 105,
        },
        TestQuery {
            subterm: "Umstrukturierung",
            target_doc_id: "doc-06",
            compound_word: "unternehmensumstrukturierungsplan",
            query_vector_seed: 106,
        },
        TestQuery {
            subterm: "Überwachung",
            target_doc_id: "doc-07",
            compound_word: "telekommunikationsueberwachungsverordnung",
            query_vector_seed: 107,
        },
        TestQuery {
            subterm: "Strategie",
            target_doc_id: "doc-08",
            compound_word: "risikomanagementstrategiepapier",
            query_vector_seed: 108,
        },
    ]
}

#[tokio::test]
async fn test_compound_split_recall_impact_evaluation() {
    let dim = 16;
    let corpus = get_corpus(dim);
    let queries = get_test_queries();

    // 1. Setup DB for Actual (Un-split) Behavior
    let dir_actual = tempdir().unwrap();
    let config_actual = MemFuseConfig {
        dimension: dim,
        max_elements: 100,
        ..Default::default()
    };
    let db_actual = MemFuse::open_with_config(dir_actual.path(), config_actual)
        .await
        .unwrap();
    let col_actual = db_actual.collection("unsplit_col").await.unwrap();

    for doc in &corpus {
        col_actual
            .insert(
                doc.id,
                &doc.embedding,
                Some(serde_json::json!({ "text": doc.raw_text })),
            )
            .await
            .unwrap();
    }

    // 2. Setup DB for Reference (Split) Behavior
    let dir_ref = tempdir().unwrap();
    let config_ref = MemFuseConfig {
        dimension: dim,
        max_elements: 100,
        ..Default::default()
    };
    let db_ref = MemFuse::open_with_config(dir_ref.path(), config_ref)
        .await
        .unwrap();
    let col_ref = db_ref.collection("split_col").await.unwrap();

    for doc in &corpus {
        col_ref
            .insert(
                doc.id,
                &doc.embedding,
                Some(serde_json::json!({ "text": doc.presplit_text })),
            )
            .await
            .unwrap();
    }

    println!("\n=========================================================================================================");
    println!("                           EMPIRICAL COMPOUND SPLIT RECALL IMPACT AUDIT MATRIX                           ");
    println!("=========================================================================================================\n");

    let weights_bm25_only = FusionWeights::new(0.0, 1.0, 0.0).unwrap();
    let weights_vec_only = FusionWeights::new(1.0, 0.0, 0.0).unwrap();
    let weights_hybrid_equal = FusionWeights::new(0.5, 0.5, 0.0).unwrap();

    println!("---------------------------------------------------------------------------------------------------------");
    println!("| Query Term     | Compound Word                              | Mode    | Rank (Split) | Rank (Bug) | Delta |");
    println!("---------------------------------------------------------------------------------------------------------");

    let mut total_queries = 0;
    let mut bm25_recalled_bug = 0;
    let mut bm25_recalled_split = 0;
    let mut hybrid_recalled_bug = 0;
    let mut hybrid_recalled_split = 0;
    let mut hybrid_rank_deltas = Vec::new();

    for q in &queries {
        let q_vec = create_embedding(q.query_vector_seed, dim);

        // A. BM25 Text Only
        #[allow(deprecated)]
        let res_actual_text = col_actual
            .hybrid_search_with_weights(q.subterm, &[], 10, None, Some(&weights_bm25_only))
            .await
            .unwrap();
        #[allow(deprecated)]
        let res_split_text = col_ref
            .hybrid_search_with_weights(q.subterm, &[], 10, None, Some(&weights_bm25_only))
            .await
            .unwrap();

        let rank_bug_text = res_actual_text
            .iter()
            .position(|r| r.id == q.target_doc_id)
            .map(|p| p + 1);
        let rank_split_text = res_split_text
            .iter()
            .position(|r| r.id == q.target_doc_id)
            .map(|p| p + 1);

        // B. Vector Only
        #[allow(deprecated)]
        let res_actual_vec = col_actual
            .hybrid_search_with_weights("", &q_vec, 10, None, Some(&weights_vec_only))
            .await
            .unwrap();
        let rank_actual_vec = res_actual_vec
            .iter()
            .position(|r| r.id == q.target_doc_id)
            .map(|p| p + 1);

        // C. Hybrid (BM25 + Vector)
        #[allow(deprecated)]
        let res_actual_hyb = col_actual
            .hybrid_search_with_weights(q.subterm, &q_vec, 10, None, Some(&weights_hybrid_equal))
            .await
            .unwrap();
        #[allow(deprecated)]
        let res_split_hyb = col_ref
            .hybrid_search_with_weights(q.subterm, &q_vec, 10, None, Some(&weights_hybrid_equal))
            .await
            .unwrap();

        let rank_bug_hyb = res_actual_hyb
            .iter()
            .position(|r| r.id == q.target_doc_id)
            .map(|p| p + 1);
        let rank_split_hyb = res_split_hyb
            .iter()
            .position(|r| r.id == q.target_doc_id)
            .map(|p| p + 1);

        total_queries += 1;
        if rank_bug_text.is_some() {
            bm25_recalled_bug += 1;
        }
        if rank_split_text.is_some() {
            bm25_recalled_split += 1;
        }
        if rank_bug_hyb.is_some() {
            hybrid_recalled_bug += 1;
        }
        if rank_split_hyb.is_some() {
            hybrid_recalled_split += 1;
        }

        let r_bug_str = rank_bug_hyb
            .map(|r| r.to_string())
            .unwrap_or_else(|| "MISS (>10)".to_string());
        let r_split_str = rank_split_hyb
            .map(|r| r.to_string())
            .unwrap_or_else(|| "MISS (>10)".to_string());
        let delta_str = match (rank_split_hyb, rank_bug_hyb) {
            (Some(s), Some(b)) => {
                let delta = b as i32 - s as i32;
                hybrid_rank_deltas.push(delta);
                format!("{:+}", delta)
            }
            (Some(_), None) => {
                hybrid_rank_deltas.push(10);
                "DROP_OUT".to_string()
            }
            _ => "0".to_string(),
        };

        let comp_truncated = if q.compound_word.len() > 42 {
            format!("{}...", &q.compound_word[..39])
        } else {
            q.compound_word.to_string()
        };

        println!(
            "| {:<14} | {:<42} | BM25    | {:<12} | {:<10} | {:<5} |",
            q.subterm,
            comp_truncated,
            rank_split_text
                .map(|r| r.to_string())
                .unwrap_or_else(|| "MISS".to_string()),
            rank_bug_text
                .map(|r| r.to_string())
                .unwrap_or_else(|| "MISS".to_string()),
            match (rank_split_text, rank_bug_text) {
                (Some(s), Some(b)) => format!("{:+}", b as i32 - s as i32),
                (Some(_), None) => "DROP_OUT".to_string(),
                _ => "0".to_string(),
            }
        );
        println!(
            "| {:<14} | {:<42} | Vector  | {:<12} | {:<10} | {:<5} |",
            "",
            "",
            rank_actual_vec
                .map(|r| r.to_string())
                .unwrap_or_else(|| "MISS".to_string()),
            rank_actual_vec
                .map(|r| r.to_string())
                .unwrap_or_else(|| "MISS".to_string()),
            "0"
        );
        println!(
            "| {:<14} | {:<42} | Hybrid  | {:<12} | {:<10} | {:<5} |",
            "", "", r_split_str, r_bug_str, delta_str
        );
        println!("---------------------------------------------------------------------------------------------------------");
    }

    println!("\nSUMMARY METRICS:");
    println!("Total Queries Tested: {}", total_queries);
    println!(
        "BM25 Recall (Bug / Un-split): {}/{} ({:.1}%)",
        bm25_recalled_bug,
        total_queries,
        (bm25_recalled_bug as f64 / total_queries as f64) * 100.0
    );
    println!(
        "BM25 Recall (Split):          {}/{} ({:.1}%)",
        bm25_recalled_split,
        total_queries,
        (bm25_recalled_split as f64 / total_queries as f64) * 100.0
    );
    println!(
        "Hybrid Recall (Bug / Un-split): {}/{} ({:.1}%)",
        hybrid_recalled_bug,
        total_queries,
        (hybrid_recalled_bug as f64 / total_queries as f64) * 100.0
    );
    println!(
        "Hybrid Recall (Split):          {}/{} ({:.1}%)",
        hybrid_recalled_split,
        total_queries,
        (hybrid_recalled_split as f64 / total_queries as f64) * 100.0
    );

    // Assertions to verify our test harness executed properly
    assert_eq!(total_queries, 8);
    assert_eq!(
        bm25_recalled_bug, 0,
        "BM25 with un-split compounds must fail 100% of subterm queries"
    );
    assert!(
        bm25_recalled_split >= 6,
        "BM25 with split compounds must recall subterm queries"
    );
}

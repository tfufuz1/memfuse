//! Empirical recall and rank impact analysis for German compound splitting failures in 4-Signal Fusion.
//! Evaluates actual vs. expected retrieval performance across BM25-only and Hybrid (Vector + Text) search.

use memfuse_core::FusionWeights;
use memfuse_db::{DistanceMetric, Language, MemFuse, MemFuseConfig};
use serde_json::json;
use tempfile::TempDir;

pub struct TestCase {
    pub query_term: &'static str,
    pub target_doc_id: &'static str,
    pub query_vector: Vec<f32>,
    pub category: &'static str,
    pub target_compound: &'static str,
}

#[tokio::test]
async fn test_german_compound_split_recall_impact() {
    let tmp = TempDir::new().expect("failed to create temp dir");

    let config = MemFuseConfig {
        dimension: 4,
        max_elements: 100,
        distance_metric: DistanceMetric::Cosine,
        ..Default::default()
    };

    let db = MemFuse::open_with_config(tmp.path(), config)
        .await
        .expect("failed to open MemFuse");

    // Collection 1: Default German morph tokenizer (Actual behavior - compounds unsplit due to dictionary gaps)
    let col_unsplit = db
        .collection_with_language("unsplit_de", Language::German)
        .await
        .expect("failed to create unsplit collection");

    // Collection 2: Pre-split reference collection (Simulated correct behavior)
    let col_split = db
        .collection_with_language("split_de", Language::German)
        .await
        .expect("failed to create split collection");

    // 20 Synthetic Corpus Documents
    // Docs 1-16 contain compounds (2 docs per compound for 8 target compounds)
    // Docs 17-20 are filler background documents containing standalone sub-terms
    let raw_docs = vec![
        (
            "doc01",
            "Der neue donaudampfschifffahrtsgesellschaftskapitaen hat sein Patent in Wien erhalten.",
            "Der neue Donau Dampf Schifffahrts Gesellschaft Kapitaen hat sein Patent in Wien erhalten.",
            vec![1.0, 0.0, 0.0, 0.0],
        ),
        (
            "doc02",
            "Ein donaudampfschifffahrtsgesellschaftskapitaen trägt auf dem Schiff hohe Verantwortung.",
            "Ein Donau Dampf Schifffahrts Gesellschaft Kapitaen trägt auf dem Schiff hohe Verantwortung.",
            vec![0.9, 0.1, 0.0, 0.0],
        ),
        (
            "doc03",
            "Im heutigen softwareentwicklungskontext sind agile Methoden unentbehrlich.",
            "Im heutigen Software Entwicklungs Kontext sind agile Methoden unentbehrlich.",
            vec![0.0, 1.0, 0.0, 0.0],
        ),
        (
            "doc04",
            "Die Architektur muss im softwareentwicklungskontext sorgfältig geplant werden.",
            "Die Architektur muss im Software Entwicklungs Kontext sorgfältig geplant werden.",
            vec![0.0, 0.9, 0.1, 0.0],
        ),
        (
            "doc05",
            "Der systemadministrator richtet die Benutzerrechte auf dem Server ein.",
            "Der System Administrator richtet die Benutzerrechte auf dem Server ein.",
            vec![0.0, 0.0, 1.0, 0.0],
        ),
        (
            "doc06",
            "Alarmmeldungen werden direkt an den zuständigen systemadministrator gesendet.",
            "Alarmmeldungen werden direkt an den zuständigen System Administrator gesendet.",
            vec![0.0, 0.0, 0.9, 0.1],
        ),
        (
            "doc07",
            "Die finanzdienstleistungsaufsichtsbehoerde prüft die Einhaltung der Solvenzregeln.",
            "Die Finanz Dienstleistungs Aufsichts Behoerde prüft die Einhaltung der Solvenzregeln.",
            vec![0.5, 0.5, 0.0, 0.0],
        ),
        (
            "doc08",
            "Banken müssen der finanzdienstleistungsaufsichtsbehoerde regelmäßige Berichte vorlegen.",
            "Banken müssen der Finanz Dienstleistungs Aufsichts Behoerde regelmäßige Berichte vorlegen.",
            vec![0.4, 0.6, 0.0, 0.0],
        ),
        (
            "doc09",
            "Unternehmen müssen die datenschutzgrundverordnungskonformitaet im Audit nachweisen.",
            "Unternehmen müssen die Datenschutz Grundverordnungs Konformitaet im Audit nachweisen.",
            vec![0.0, 0.5, 0.5, 0.0],
        ),
        (
            "doc10",
            "Maßnahmen zur datenschutzgrundverordnungskonformitaet betreffen alle IT-Systeme.",
            "Maßnahmen zur Datenschutz Grundverordnungs Konformitaet betreffen alle IT-Systeme.",
            vec![0.0, 0.4, 0.6, 0.0],
        ),
        (
            "doc11",
            "Das neue gesellschaftsrechtsreformgesetz ändert die Vorgaben für die GmbH.",
            "Das neue Gesellschafts Rechts Reform Gesetz ändert die Vorgaben für die GmbH.",
            vec![0.5, 0.0, 0.5, 0.0],
        ),
        (
            "doc12",
            "Gutachten zum gesellschaftsrechtsreformgesetz liegen dem Ausschuss vor.",
            "Gutachten zum Gesellschafts Rechts Reform Gesetz liegen dem Ausschuss vor.",
            vec![0.6, 0.0, 0.4, 0.0],
        ),
        (
            "doc13",
            "Ein zertifiziertes informationssicherheitsmanagementsystem schützt vor Cyberrisiken.",
            "Ein zertifiziertes Informations Sicherheits Managementsystem schützt vor Cyberrisiken.",
            vec![0.0, 0.0, 0.5, 0.5],
        ),
        (
            "doc14",
            "Anforderungen an das informationssicherheitsmanagementsystem werden jährlich überprüft.",
            "Anforderungen an das Informations Sicherheits Managementsystem werden jährlich überprüft.",
            vec![0.0, 0.0, 0.4, 0.6],
        ),
        (
            "doc15",
            "Eine gültige kapitalertragsteuerbefreiungsbescheinigung muss der Bank vorliegen.",
            "Eine gültige Kapital Ertrag Steuer Befreiungs Bescheinigung muss der Bank vorliegen.",
            vec![0.5, 0.0, 0.0, 0.5],
        ),
        (
            "doc16",
            "Der Antrag auf kapitalertragsteuerbefreiungsbescheinigung wurde beim Finanzamt eingereicht.",
            "Der Antrag auf Kapital Ertrag Steuer Befreiungs Bescheinigung wurde beim Finanzamt eingereicht.",
            vec![0.6, 0.0, 0.0, 0.4],
        ),
        // Filler Docs 17-20 with standalone keywords
        (
            "doc17",
            "Der Kapitän der Marine besprach die Fahrt und das Schiff mit dem Offizier.",
            "Der Kapitän der Marine besprach die Fahrt und das Schiff mit dem Offizier.",
            vec![0.1, 0.1, 0.1, 0.7],
        ),
        (
            "doc18",
            "Moderner Software Code benötigt gute Betreuung durch erfahrene Entwickler.",
            "Moderner Software Code benötigt gute Betreuung durch erfahrene Entwickler.",
            vec![0.1, 0.7, 0.1, 0.1],
        ),
        (
            "doc19",
            "Jede Behörde verlangt eine Bescheinigung über Steuer und Finanzen.",
            "Jede Behörde verlangt eine Bescheinigung über Steuer und Finanzen.",
            vec![0.7, 0.1, 0.1, 0.1],
        ),
        (
            "doc20",
            "Das Gesetz regelt die Aufsicht über das System und dessen Sicherheit.",
            "Das Gesetz regelt die Aufsicht über das System und dessen Sicherheit.",
            vec![0.1, 0.1, 0.7, 0.1],
        ),
    ];

    for (id, text_unsplit, text_split, vec) in &raw_docs {
        col_unsplit
            .insert(id, vec, Some(json!({ "text": text_unsplit })))
            .await
            .expect("insert unsplit failed");
        col_split
            .insert(id, vec, Some(json!({ "text": text_split })))
            .await
            .expect("insert split failed");
    }

    let test_cases = vec![
        TestCase {
            query_term: "Kapitäne",
            target_doc_id: "doc01",
            query_vector: vec![1.0, 0.0, 0.0, 0.0],
            category: "Marine/Extreme",
            target_compound: "donaudampfschifffahrtsgesellschaftskapitaen",
        },
        TestCase {
            query_term: "Kapitaen",
            target_doc_id: "doc01",
            query_vector: vec![1.0, 0.0, 0.0, 0.0],
            category: "Marine/Extreme",
            target_compound: "donaudampfschifffahrtsgesellschaftskapitaen",
        },
        TestCase {
            query_term: "Software",
            target_doc_id: "doc03",
            query_vector: vec![0.0, 1.0, 0.0, 0.0],
            category: "IT/Hybrid",
            target_compound: "softwareentwicklungskontext",
        },
        TestCase {
            query_term: "Kontext",
            target_doc_id: "doc03",
            query_vector: vec![0.0, 1.0, 0.0, 0.0],
            category: "IT/Hybrid",
            target_compound: "softwareentwicklungskontext",
        },
        TestCase {
            query_term: "Administrator",
            target_doc_id: "doc05",
            query_vector: vec![0.0, 0.0, 1.0, 0.0],
            category: "IT/Compound",
            target_compound: "systemadministrator",
        },
        TestCase {
            query_term: "Aufsicht",
            target_doc_id: "doc07",
            query_vector: vec![0.5, 0.5, 0.0, 0.0],
            category: "Finance/Regulatory",
            target_compound: "finanzdienstleistungsaufsichtsbehoerde",
        },
        TestCase {
            query_term: "Behoerde",
            target_doc_id: "doc07",
            query_vector: vec![0.5, 0.5, 0.0, 0.0],
            category: "Finance/Regulatory",
            target_compound: "finanzdienstleistungsaufsichtsbehoerde",
        },
        TestCase {
            query_term: "Datenschutz",
            target_doc_id: "doc09",
            query_vector: vec![0.0, 0.5, 0.5, 0.0],
            category: "Legal/Compliance",
            target_compound: "datenschutzgrundverordnungskonformitaet",
        },
        TestCase {
            query_term: "Konformitaet",
            target_doc_id: "doc09",
            query_vector: vec![0.0, 0.5, 0.5, 0.0],
            category: "Legal/Compliance",
            target_compound: "datenschutzgrundverordnungskonformitaet",
        },
        TestCase {
            query_term: "Gesellschaft",
            target_doc_id: "doc11",
            query_vector: vec![0.5, 0.0, 0.5, 0.0],
            category: "Legal/Corporate",
            target_compound: "gesellschaftsrechtsreformgesetz",
        },
        TestCase {
            query_term: "Reform",
            target_doc_id: "doc11",
            query_vector: vec![0.5, 0.0, 0.5, 0.0],
            category: "Legal/Corporate",
            target_compound: "gesellschaftsrechtsreformgesetz",
        },
        TestCase {
            query_term: "Sicherheit",
            target_doc_id: "doc13",
            query_vector: vec![0.0, 0.0, 0.5, 0.5],
            category: "IT/Security",
            target_compound: "informationssicherheitsmanagementsystem",
        },
        TestCase {
            query_term: "Steuer",
            target_doc_id: "doc15",
            query_vector: vec![0.5, 0.0, 0.0, 0.5],
            category: "Finance/Tax",
            target_compound: "kapitalertragsteuerbefreiungsbescheinigung",
        },
        TestCase {
            query_term: "Bescheinigung",
            target_doc_id: "doc15",
            query_vector: vec![0.5, 0.0, 0.0, 0.5],
            category: "Finance/Tax",
            target_compound: "kapitalertragsteuerbefreiungsbescheinigung",
        },
    ];

    println!("\n=== EMPIRICAL EVALUATION: GERMAN COMPOUND SPLIT RECALL IMPACT ===");
    println!(
        "{:<15} | {:<12} | {:<12} | {:<12} | {:<12} | {:<6}",
        "Query Sub-Term",
        "BM25 (Split)",
        "BM25 (Unsplit)",
        "Hybr (Split)",
        "Hybr (Unsplit)",
        "Delta"
    );
    println!("{}", "-".repeat(80));

    let mut bm25_unsplit_misses = 0;
    let mut hybrid_unsplit_misses = 0;

    let bm25_weights = FusionWeights::new(0.0, 1.0, 0.0).unwrap();
    let hybrid_weights = FusionWeights::new(0.5, 0.5, 0.0).unwrap();

    for tc in &test_cases {
        // 1. BM25-only Search on Split collection
        let q_split_bm25 = col_split
            .query()
            .text(tc.query_term)
            .vector(vec![0.0, 0.0, 0.0, 0.0])
            .fusion_weights(bm25_weights.clone())
            .k(10)
            .execute()
            .await
            .expect("bm25 split search failed");

        let rank_split_bm25 = q_split_bm25
            .iter()
            .position(|r| r.id == tc.target_doc_id)
            .map(|p| (p + 1) as i32)
            .unwrap_or(999);

        // 2. BM25-only Search on Unsplit collection
        let q_unsplit_bm25 = col_unsplit
            .query()
            .text(tc.query_term)
            .vector(vec![0.0, 0.0, 0.0, 0.0])
            .fusion_weights(bm25_weights.clone())
            .k(10)
            .execute()
            .await
            .expect("bm25 unsplit search failed");

        let rank_unsplit_bm25 = q_unsplit_bm25
            .iter()
            .position(|r| r.id == tc.target_doc_id)
            .map(|p| (p + 1) as i32)
            .unwrap_or(999);

        // 3. Hybrid Search on Split collection
        let q_split_hybr = col_split
            .query()
            .text(tc.query_term)
            .vector(tc.query_vector.clone())
            .fusion_weights(hybrid_weights.clone())
            .k(10)
            .execute()
            .await
            .expect("hybrid split search failed");

        let rank_split_hybr = q_split_hybr
            .iter()
            .position(|r| r.id == tc.target_doc_id)
            .map(|p| (p + 1) as i32)
            .unwrap_or(999);

        // 4. Hybrid Search on Unsplit collection
        let q_unsplit_hybr = col_unsplit
            .query()
            .text(tc.query_term)
            .vector(tc.query_vector.clone())
            .fusion_weights(hybrid_weights.clone())
            .k(10)
            .execute()
            .await
            .expect("hybrid unsplit search failed");

        let rank_unsplit_hybr = q_unsplit_hybr
            .iter()
            .position(|r| r.id == tc.target_doc_id)
            .map(|p| (p + 1) as i32)
            .unwrap_or(999);

        if rank_unsplit_bm25 == 999 {
            bm25_unsplit_misses += 1;
        }
        if rank_unsplit_hybr == 999 {
            hybrid_unsplit_misses += 1;
        }

        let delta_bm25 = if rank_unsplit_bm25 == 999 {
            999 - rank_split_bm25
        } else {
            rank_unsplit_bm25 - rank_split_bm25
        };

        let str_rank = |r: i32| {
            if r == 999 {
                "NOT IN TOP10".to_string()
            } else {
                format!("#{}", r)
            }
        };

        println!(
            "{:<15} | {:<12} | {:<12} | {:<12} | {:<12} | {:<6}",
            tc.query_term,
            str_rank(rank_split_bm25),
            str_rank(rank_unsplit_bm25),
            str_rank(rank_split_hybr),
            str_rank(rank_unsplit_hybr),
            if delta_bm25 >= 900 {
                ">900".to_string()
            } else {
                format!("+{}", delta_bm25)
            }
        );
    }

    println!("{}", "-".repeat(80));
    println!("Total queries evaluated: {}", test_cases.len());
    println!(
        "BM25 Top-10 recall drop (Unsplit): {} / {} queries ({:.1}%)",
        bm25_unsplit_misses,
        test_cases.len(),
        (bm25_unsplit_misses as f64 / test_cases.len() as f64) * 100.0
    );
    println!(
        "Hybrid Top-10 recall drop (Unsplit): {} / {} queries ({:.1}%)",
        hybrid_unsplit_misses,
        test_cases.len(),
        (hybrid_unsplit_misses as f64 / test_cases.len() as f64) * 100.0
    );

    // Verifications
    assert_eq!(
        bm25_unsplit_misses,
        test_cases.len(),
        "BM25-only recall must drop to 0% for sub-term queries when compound splitting fails"
    );
}

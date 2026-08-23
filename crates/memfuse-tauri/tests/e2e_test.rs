//! End-to-End-Test: Simuliert den vollständigen Nutzerpfad von MemFuse Brain
//! ohne echtes Ollama (Mock-Embedder), aber mit echter Storage-Engine.

use memfuse_db::{MemFuse, MemFuseConfig};
use memfuse_tauri_lib::ingestion::{EmbeddingProvider, IngestionPipeline};
use std::sync::Arc;
use tempfile::tempdir;

/// Deterministischer Test-Embedder: erzeugt Vektoren basierend auf
/// enthaltenen Schlüsselwörtern, sodass thematisch ähnliche Texte auch
/// tatsächlich nahe beieinander liegende Vektoren erhalten (nicht rein
/// zufällig — sonst wäre der Vektor-Signal-Test bedeutungslos).
struct KeywordEmbedder;

#[async_trait::async_trait]
impl EmbeddingProvider for KeywordEmbedder {
    async fn embed(&self, text: &str) -> memfuse_core::Result<Vec<f32>> {
        let lower = text.to_lowercase();
        let dim_urlaub = if lower.contains("urlaub") { 1.0 } else { 0.0 };
        let dim_gehalt = if lower.contains("gehalt") { 1.0 } else { 0.0 };
        let dim_lager = if lower.contains("lager") { 1.0 } else { 0.0 };
        let dim_generic = 0.1;
        Ok(vec![dim_urlaub, dim_gehalt, dim_lager, dim_generic])
    }
}

#[tokio::test]
async fn test_full_pipeline_ingest_search_and_chat_context() {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("e2e_db");
    let docs_dir = dir.path().join("docs");
    std::fs::create_dir_all(&docs_dir).unwrap();

    // ── 1. Test-Dokumente anlegen ────────────────────────────────────────
    std::fs::write(
        docs_dir.join("urlaub.md"),
        "# Urlaubsantrag\n\nMitarbeiter können ihren Urlaubsantrag über \
         das interne Portal stellen. Die Genehmigung erfolgt durch die \
         direkte Führungskraft innerhalb von 3 Werktagen.",
    )
    .unwrap();

    std::fs::write(
        docs_dir.join("gehalt.md"),
        "# Gehaltsabrechnung\n\nDie Gehaltsabrechnung erfolgt monatlich \
         zum 25. Kalendertag. Bei Fragen wenden Sie sich an die Personalabteilung.",
    )
    .unwrap();

    std::fs::write(
        docs_dir.join("lager.md"),
        "# Lagerbestand\n\nDer aktuelle Lagerbestand wird wöchentlich \
         inventarisiert. Mindestbestände sind im ERP-System hinterlegt.",
    )
    .unwrap();

    // ── 2. Datenbank öffnen, Collection erstellen ────────────────────────
    let config = MemFuseConfig {
        dimension: 4,
        ..Default::default()
    };
    let db = MemFuse::open_with_config(&db_path, config.clone())
        .await
        .expect("DB öffnen");
    let collection = db.collection("hr_docs").await.expect("Collection erstellen");

    // ── 3. Ordner importieren ─────────────────────────────────────────────
    let embedder: Arc<dyn EmbeddingProvider> = Arc::new(KeywordEmbedder);
    let pipeline = IngestionPipeline::new(embedder.clone());
    let reports = pipeline
        .ingest_folder(&docs_dir, &collection)
        .await
        .expect("Ordner-Import");

    assert_eq!(reports.len(), 3, "Alle 3 Testdokumente sollten verarbeitet werden");
    let total_chunks: usize = reports.iter().map(|r| r.chunks_created).sum();
    assert!(total_chunks >= 3, "Mindestens 1 Chunk pro Dokument erwartet");
    for report in &reports {
        assert!(
            report.errors.is_empty(),
            "Keine Fehler erwartet: {:?}",
            report.errors
        );
    }

    // ── 4. Hybrid-Suche: Urlaubsfrage sollte urlaub.md finden ────────────
    let query = "Wie stelle ich einen Urlaubsantrag?";
    let query_vector = embedder.embed(query).await.unwrap();

    let results = collection
        .hybrid_search(query, &query_vector, 5, None)
        .await
        .expect("Hybrid-Suche");

    assert!(!results.is_empty(), "Suche sollte Ergebnisse liefern");

    let top_result_mentions_urlaub = results.iter().any(|r| {
        r.metadata
            .as_ref()
            .and_then(|m| m.get("text"))
            .and_then(|t| t.as_str())
            .map(|t| t.to_lowercase().contains("urlaub"))
            .unwrap_or(false)
    });
    assert!(
        top_result_mentions_urlaub,
        "Die Urlaubsfrage sollte mindestens ein Ergebnis mit 'Urlaub' im Text finden"
    );

    // ── 5. Negativ-Test: Lager-Frage sollte NICHT das Gehalt-Dokument
    //    als Top-Treffer liefern (Signal-Trennschärfe prüfen) ─────────────
    let lager_query = "Wie hoch ist der aktuelle Lagerbestand?";
    let lager_vector = embedder.embed(lager_query).await.unwrap();
    let lager_results = collection
        .hybrid_search(lager_query, &lager_vector, 3, None)
        .await
        .expect("Lager-Suche");

    assert!(!lager_results.is_empty());
    let top_lager_hit = &lager_results[0];
    let top_text = top_lager_hit
        .metadata
        .as_ref()
        .and_then(|m| m.get("text"))
        .and_then(|t| t.as_str())
        .unwrap_or("");
    assert!(
        top_text.to_lowercase().contains("lager"),
        "Top-Treffer für Lager-Frage sollte tatsächlich vom Lager-Dokument stammen, war aber: {top_text}"
    );

    // ── 6. Persistenz über Neustart prüfen (End-to-End auf Storage-Ebene) ─
    db.close().await.expect("DB schließen");
    let db2 = MemFuse::open_with_config(&db_path, config)
        .await
        .expect("DB erneut öffnen");
    let collection2 = db2
        .collection("hr_docs")
        .await
        .expect("Collection erneut öffnen");
    let count = collection2.len().await;
    assert!(
        count >= 3,
        "Dokumente müssen nach Neustart noch vorhanden sein"
    );
}

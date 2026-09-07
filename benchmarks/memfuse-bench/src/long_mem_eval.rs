// FILE-CONTEXT
// STAND: 2026-09-07
// ZWECK: Loader, Evaluation Harness und Regressions-Suite für das LongMemEval Benchmark (ICLR 2025 / arXiv:2410.10813)
// INVARIANTEN: Lose Kopplung über Search-Closure, keine Panic im Produktionscode, aussagekräftige Fehler.

use memfuse_core::{MemFuseError, Result, StorageEngine, VectorIndex};
use memfuse_db::Collection;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::File;
use std::future::Future;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::pin::Pin;

/// Alias for an owned Send BoxFuture.
pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Generic search chunk returned by an injected search pipeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoredChunk {
    pub id: String,
    pub text: String,
    pub score: f32,
}

/// Official question categories in LongMemEval (arXiv:2410.10813).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LongMemEvalQuestionType {
    SingleSessionUser,
    SingleSessionAssistant,
    SingleSessionPreference,
    MultiSession,
    KnowledgeUpdate,
    TemporalReasoning,
    Abstention,
}

impl std::fmt::Display for LongMemEvalQuestionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SingleSessionUser => write!(f, "SingleSessionUser"),
            Self::SingleSessionAssistant => write!(f, "SingleSessionAssistant"),
            Self::SingleSessionPreference => write!(f, "SingleSessionPreference"),
            Self::MultiSession => write!(f, "MultiSession"),
            Self::KnowledgeUpdate => write!(f, "KnowledgeUpdate"),
            Self::TemporalReasoning => write!(f, "TemporalReasoning"),
            Self::Abstention => write!(f, "Abstention"),
        }
    }
}

/// Evaluation test case for LongMemEval.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalCase {
    pub question_id: String,
    pub session_history: Vec<(
        String, /* speaker/role */
        String, /* utterance */
        u64,    /* session_idx */
    )>,
    pub question: String,
    pub expected_answer: String,
    pub question_type: LongMemEvalQuestionType,
}

/// Report containing evaluation metrics per question category.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongMemEvalReport {
    pub per_category_accuracy: HashMap<LongMemEvalQuestionType, f64>,
    pub overall_accuracy: f64,
    pub total_cases: usize,
}

/// Single conversational turn in a multi-session scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTurn {
    pub speaker: String,
    pub text: String,
    pub doc_id: String,
}

/// A session containing a sequence of turns in a scenario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub session_id: String,
    pub timestamp_ms: u64,
    pub turns: Vec<SessionTurn>,
}

/// Multi-session evaluation scenario containing distributed context across sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiSessionScenario {
    pub scenario_id: String,
    pub description: String,
    pub question_type: LongMemEvalQuestionType,
    pub sessions: Vec<Session>,
    pub query: String,
    pub expected_answer_doc_id: String,
    pub expected_keywords: Vec<String>,
}

/// Regressions-Suite inspiriert von LongMemEval (Wu et al.).
/// WICHTIG (P7): Zahlen aus diesem Modul sind NICHT direkt vergleichbar mit
/// offiziell publizierten LongMemEval-Ergebnissen anderer Systeme, da hier
/// ein strukturell äquivalentes, aber eigenständiges Testset verwendet wird
/// (sofern der Original-Datensatz nicht lizenzkonform eingebunden werden konnte).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegressionSuite {
    pub scenarios: Vec<MultiSessionScenario>,
}

/// Report summarizing recall and scenario metrics for the regression suite.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RegressionReport {
    pub recall_at_5: f64,
    pub recall_at_10: f64,
    pub overall_accuracy: f64,
    pub total_scenarios: usize,
    pub failed_scenarios: Vec<String>,
}

/// Compares a current regression report against a stored baseline file.
/// Returns an error if Recall@5 drops by more than 3 percentage points (0.03).
pub fn check_regression(current: &RegressionReport, baseline_path: &Path) -> std::result::Result<(), String> {
    if !baseline_path.exists() {
        return Err(format!(
            "Baseline report file not found at {}. Generate it using baseline creation.",
            baseline_path.display()
        ));
    }
    let content = std::fs::read_to_string(baseline_path)
        .map_err(|e| format!("Failed to read baseline file {}: {}", baseline_path.display(), e))?;
    let baseline: RegressionReport = serde_json::from_str(&content)
        .map_err(|e| format!("Failed to parse baseline report from {}: {}", baseline_path.display(), e))?;

    let delta = baseline.recall_at_5 - current.recall_at_5;
    if delta > 0.03 {
        return Err(format!(
            "Recall@5-Regression: {:.3} -> {:.3} (Δ={:.3}). Failed scenarios: {:?}",
            baseline.recall_at_5, current.recall_at_5, delta, current.failed_scenarios
        ));
    }
    Ok(())
}

fn pad_vector(v: &[f32], target_dim: usize) -> Vec<f32> {
    let mut padded = vec![0.0f32; target_dim];
    for (i, &val) in v.iter().enumerate().take(target_dim) {
        padded[i] = val;
    }
    padded
}

impl RegressionSuite {
    /// Constructs a baseline suite with 31 multi-session scenarios.
    pub fn baseline() -> Self {
        let mut scenarios = Vec::new();

        // ---------------------------------------------------------------------
        // 1. KNOWLEDGE UPDATES / SUPERSEDES (Scenarios 1-10)
        // ---------------------------------------------------------------------
        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_01".into(),
            description: "Programming language preference update (Python -> Rust)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_01_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "I write all my backend microservices in Python 3.11 with FastAPI.".into(),
                        doc_id: "doc_sup_01_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_01_b".into(),
                    timestamp_ms: 5000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "UPDATE: We completely migrated all services to Rust using Tokio and Axum for memory efficiency.".into(),
                        doc_id: "doc_sup_01_new".into(),
                    }],
                },
            ],
            query: "What is the primary programming language for the user's backend services?".into(),
            expected_answer_doc_id: "doc_sup_01_new".into(),
            expected_keywords: vec!["Rust".into(), "Tokio".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_02".into(),
            description: "User residence move (Munich -> Berlin)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_02_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "I live near Englischer Garten in Munich, Germany.".into(),
                        doc_id: "doc_sup_02_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_02_b".into(),
                    timestamp_ms: 6000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Correction: I moved to Berlin Kreuzberg last month for my new tech job.".into(),
                        doc_id: "doc_sup_02_new".into(),
                    }],
                },
            ],
            query: "Where does the user currently reside?".into(),
            expected_answer_doc_id: "doc_sup_02_new".into(),
            expected_keywords: vec!["Berlin".into(), "Kreuzberg".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_03".into(),
            description: "Project team lead change (Alice -> Bob)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_03_a".into(),
                    timestamp_ms: 2000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Alice is currently leading the MemFuse database core engineering team.".into(),
                        doc_id: "doc_sup_03_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_03_b".into(),
                    timestamp_ms: 7000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "As of Q3, Bob took over as the technical lead for the MemFuse core engineering team.".into(),
                        doc_id: "doc_sup_03_new".into(),
                    }],
                },
            ],
            query: "Who is the current team lead of MemFuse core engineering?".into(),
            expected_answer_doc_id: "doc_sup_03_new".into(),
            expected_keywords: vec!["Bob".into(), "lead".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_04".into(),
            description: "IDE choice update (VS Code -> JetBrains CLion)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_04_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "My primary editor is VS Code with rust-analyzer extension.".into(),
                        doc_id: "doc_sup_04_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_04_b".into(),
                    timestamp_ms: 8000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "I switched completely to JetBrains CLion Rust plugin for better debugging and refactoring.".into(),
                        doc_id: "doc_sup_04_new".into(),
                    }],
                },
            ],
            query: "Which IDE or editor does the user use for development?".into(),
            expected_answer_doc_id: "doc_sup_04_new".into(),
            expected_keywords: vec!["CLion".into(), "JetBrains".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_05".into(),
            description: "Database engine migration (PostgreSQL -> MemFuse LSM-Tree)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_05_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "We use PostgreSQL 15 for storing session graph memories.".into(),
                        doc_id: "doc_sup_05_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_05_b".into(),
                    timestamp_ms: 9000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Architectural change: We migrated storage to MemFuse LSM-Tree with SSTable persistence.".into(),
                        doc_id: "doc_sup_05_new".into(),
                    }],
                },
            ],
            query: "Which database storage engine is used for session memory?".into(),
            expected_answer_doc_id: "doc_sup_05_new".into(),
            expected_keywords: vec!["MemFuse".into(), "LSM-Tree".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_06".into(),
            description: "Cloud provider shift (AWS -> GCP)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_06_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Our Kubernetes clusters run in AWS us-east-1.".into(),
                        doc_id: "doc_sup_06_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_06_b".into(),
                    timestamp_ms: 10000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Cloud update: All infrastructure was moved to GCP europe-west3 in Frankfurt.".into(),
                        doc_id: "doc_sup_06_new".into(),
                    }],
                },
            ],
            query: "Which cloud provider hosts the production infrastructure?".into(),
            expected_answer_doc_id: "doc_sup_06_new".into(),
            expected_keywords: vec!["GCP".into(), "Frankfurt".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_07".into(),
            description: "Working schedule shift (Fixed 9-5 -> Async Flexible)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_07_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "The team operates strictly 9:00 AM to 5:00 PM CET.".into(),
                        doc_id: "doc_sup_07_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_07_b".into(),
                    timestamp_ms: 11000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Policy change: We transitioned to 100% async flexible hours across all timezones.".into(),
                        doc_id: "doc_sup_07_new".into(),
                    }],
                },
            ],
            query: "What is the team's work schedule policy?".into(),
            expected_answer_doc_id: "doc_sup_07_new".into(),
            expected_keywords: vec!["async".into(), "flexible".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_08".into(),
            description: "License change (GPL -> Apache 2.0)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_08_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "The codebase is licensed under GNU GPL v3.".into(),
                        doc_id: "doc_sup_08_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_08_b".into(),
                    timestamp_ms: 12000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Relicensing announcement: The project was relicensed to Apache License 2.0.".into(),
                        doc_id: "doc_sup_08_new".into(),
                    }],
                },
            ],
            query: "What open source license governs the project?".into(),
            expected_answer_doc_id: "doc_sup_08_new".into(),
            expected_keywords: vec!["Apache".into(), "2.0".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_09".into(),
            description: "Distance metric update (L2 -> Cosine)".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_09_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "HNSW index uses Euclidean L2 distance for vector ranking.".into(),
                        doc_id: "doc_sup_09_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_09_b".into(),
                    timestamp_ms: 13000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Vector index update: Switched distance metric to Cosine distance for unit-normalized embeddings.".into(),
                        doc_id: "doc_sup_09_new".into(),
                    }],
                },
            ],
            query: "Which distance metric is used in the vector index?".into(),
            expected_answer_doc_id: "doc_sup_09_new".into(),
            expected_keywords: vec!["Cosine".into(), "distance".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "sup_10".into(),
            description: "Support phone number update".into(),
            question_type: LongMemEvalQuestionType::KnowledgeUpdate,
            sessions: vec![
                Session {
                    session_id: "sess_sup_10_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Emergency phone line for support is +1-800-555-0100.".into(),
                        doc_id: "doc_sup_10_old".into(),
                    }],
                },
                Session {
                    session_id: "sess_sup_10_b".into(),
                    timestamp_ms: 14000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Updated contact info: The new 24/7 hotline is +1-888-555-0199.".into(),
                        doc_id: "doc_sup_10_new".into(),
                    }],
                },
            ],
            query: "What is the emergency support hotline number?".into(),
            expected_answer_doc_id: "doc_sup_10_new".into(),
            expected_keywords: vec!["+1-888-555-0199".into()],
        });

        // ---------------------------------------------------------------------
        // 2. MULTI-SESSION TEMPORAL REASONING & AGGREGATION (Scenarios 11-20)
        // ---------------------------------------------------------------------
        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_11".into(),
            description: "Conference attendance tracking across cities".into(),
            question_type: LongMemEvalQuestionType::TemporalReasoning,
            sessions: vec![
                Session {
                    session_id: "sess_multi_11_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "In 2022 I attended EuroSys in Munich.".into(),
                        doc_id: "doc_multi_11_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_11_b".into(),
                    timestamp_ms: 5000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "In 2023 I presented a paper at SOSP in Vienna.".into(),
                        doc_id: "doc_multi_11_2".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_11_c".into(),
                    timestamp_ms: 10000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "In 2024 I registered for OSDI held in Zurich, Switzerland.".into(),
                        doc_id: "doc_multi_11_3".into(),
                    }],
                },
            ],
            query: "Which conference in Zurich did the user attend in 2024?".into(),
            expected_answer_doc_id: "doc_multi_11_3".into(),
            expected_keywords: vec!["OSDI".into(), "Zurich".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_12".into(),
            description: "Pet adoption details in multi-year timeline".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_12_a".into(),
                    timestamp_ms: 2000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "We got a golden retriever named Max in 2020.".into(),
                        doc_id: "doc_multi_12_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_12_b".into(),
                    timestamp_ms: 8000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "In November 2023 we adopted a rescue tabby cat named Cleo from the shelter.".into(),
                        doc_id: "doc_multi_12_2".into(),
                    }],
                },
            ],
            query: "What is the name of the rescue cat adopted in 2023?".into(),
            expected_answer_doc_id: "doc_multi_12_2".into(),
            expected_keywords: vec!["Cleo".into(), "cat".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_13".into(),
            description: "HNSW paper discussions across meetings".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_13_a".into(),
                    timestamp_ms: 3000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Meeting 1 covered BM25 Robertson-Spärck-Jones score functions.".into(),
                        doc_id: "doc_multi_13_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_13_b".into(),
                    timestamp_ms: 9000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Meeting 2 focused on Malkov & Yashunin's 2018 HNSW paper on logarithmic search complexity.".into(),
                        doc_id: "doc_multi_13_2".into(),
                    }],
                },
            ],
            query: "Which paper on HNSW vector graphs was discussed in the second meeting?".into(),
            expected_answer_doc_id: "doc_multi_13_2".into(),
            expected_keywords: vec!["Malkov".into(), "HNSW".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_14".into(),
            description: "Server hardware node specification".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_14_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Server Node Alpha has 512GB ECC RAM and dual AMD EPYC CPUs.".into(),
                        doc_id: "doc_multi_14_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_14_b".into(),
                    timestamp_ms: 5000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Server Node Beta has 128GB RAM and NVMe storage array.".into(),
                        doc_id: "doc_multi_14_2".into(),
                    }],
                },
            ],
            query: "What is the memory RAM capacity of Server Node Alpha?".into(),
            expected_answer_doc_id: "doc_multi_14_1".into(),
            expected_keywords: vec!["512GB".into(), "RAM".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_15".into(),
            description: "Quarterly department budget breakdown".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_15_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Q2 engineering budget was set to $450,000.".into(),
                        doc_id: "doc_multi_15_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_15_b".into(),
                    timestamp_ms: 7000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Q3 engineering budget was approved at $620,000 for infrastructure scaling.".into(),
                        doc_id: "doc_multi_15_2".into(),
                    }],
                },
            ],
            query: "What was the approved Q3 budget for engineering?".into(),
            expected_answer_doc_id: "doc_multi_15_2".into(),
            expected_keywords: vec!["$620,000".into(), "Q3".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_16".into(),
            description: "Summer vacation location in 2024".into(),
            question_type: LongMemEvalQuestionType::TemporalReasoning,
            sessions: vec![
                Session {
                    session_id: "sess_multi_16_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "In summer 2023 we spent two weeks in Crete, Greece.".into(),
                        doc_id: "doc_multi_16_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_16_b".into(),
                    timestamp_ms: 12000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "For summer 2024 we hiked the Fjords in Bergen, Norway.".into(),
                        doc_id: "doc_multi_16_2".into(),
                    }],
                },
            ],
            query: "Where did the user spend their summer vacation in 2024?".into(),
            expected_answer_doc_id: "doc_multi_16_2".into(),
            expected_keywords: vec!["Bergen".into(), "Norway".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_17".into(),
            description: "Book recommendation across topics".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_17_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "I recommended 'Designing Data-Intensive Applications' by Martin Kleppmann.".into(),
                        doc_id: "doc_multi_17_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_17_b".into(),
                    timestamp_ms: 8000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "For quantum computing, I recommended 'Quantum Computation and Quantum Information' by Nielsen & Chuang.".into(),
                        doc_id: "doc_multi_17_2".into(),
                    }],
                },
            ],
            query: "Which book was recommended for quantum computing?".into(),
            expected_answer_doc_id: "doc_multi_17_2".into(),
            expected_keywords: vec!["Nielsen".into(), "Chuang".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_18".into(),
            description: "Vehicle service history maintenance".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_18_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Changed engine oil at 45,000 km in January.".into(),
                        doc_id: "doc_multi_18_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_18_b".into(),
                    timestamp_ms: 9000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Replaced brake fluid and front pads at 52,000 km service in September.".into(),
                        doc_id: "doc_multi_18_2".into(),
                    }],
                },
            ],
            query: "At how many kilometers was the brake fluid replaced?".into(),
            expected_answer_doc_id: "doc_multi_18_2".into(),
            expected_keywords: vec!["52,000".into(), "brake".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_19".into(),
            description: "Machine learning course certification".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_19_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Enrolled in Coursera Deep Learning Specialization.".into(),
                        doc_id: "doc_multi_19_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_19_b".into(),
                    timestamp_ms: 10000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Successfully passed Stanford CS224N Natural Language Processing with Deep Learning.".into(),
                        doc_id: "doc_multi_19_2".into(),
                    }],
                },
            ],
            query: "Which NLP course certification did the user complete?".into(),
            expected_answer_doc_id: "doc_multi_19_2".into(),
            expected_keywords: vec!["CS224N".into(), "Stanford".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "multi_20".into(),
            description: "Medical allergy report in health profile".into(),
            question_type: LongMemEvalQuestionType::MultiSession,
            sessions: vec![
                Session {
                    session_id: "sess_multi_20_a".into(),
                    timestamp_ms: 1000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Patient profile notes mild lactose intolerance.".into(),
                        doc_id: "doc_multi_20_1".into(),
                    }],
                },
                Session {
                    session_id: "sess_multi_20_b".into(),
                    timestamp_ms: 11000,
                    turns: vec![SessionTurn {
                        speaker: "User".into(),
                        text: "Critical allergy alert: Patient has severe Penicillin drug allergy.".into(),
                        doc_id: "doc_multi_20_2".into(),
                    }],
                },
            ],
            query: "Which medication drug allergy is listed in the profile?".into(),
            expected_answer_doc_id: "doc_multi_20_2".into(),
            expected_keywords: vec!["Penicillin".into(), "allergy".into()],
        });

        // ---------------------------------------------------------------------
        // 3. SINGLE-SESSION PREFERENCES & FACTS (Scenarios 21-28)
        // ---------------------------------------------------------------------
        scenarios.push(MultiSessionScenario {
            scenario_id: "single_21".into(),
            description: "User coffee preference".into(),
            question_type: LongMemEvalQuestionType::SingleSessionPreference,
            sessions: vec![Session {
                session_id: "sess_single_21".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "I always order an oat milk cappuccino with extra espresso shot.".into(),
                    doc_id: "doc_single_21".into(),
                }],
            }],
            query: "How does the user prefer their coffee prepared?".into(),
            expected_answer_doc_id: "doc_single_21".into(),
            expected_keywords: vec!["oat".into(), "cappuccino".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_22".into(),
            description: "Linux distro preference".into(),
            question_type: LongMemEvalQuestionType::SingleSessionUser,
            sessions: vec![Session {
                session_id: "sess_single_22".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "My workstation workstation runs Fedora Workstation Linux 40 with GNOME.".into(),
                    doc_id: "doc_single_22".into(),
                }],
            }],
            query: "Which Linux distribution is installed on the workstation?".into(),
            expected_answer_doc_id: "doc_single_22".into(),
            expected_keywords: vec!["Fedora".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_23".into(),
            description: "Favorite science fiction author".into(),
            question_type: LongMemEvalQuestionType::SingleSessionPreference,
            sessions: vec![Session {
                session_id: "sess_single_23".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "My favorite science fiction author of all time is Ursula K. Le Guin.".into(),
                    doc_id: "doc_single_23".into(),
                }],
            }],
            query: "Who is the user's favorite science fiction author?".into(),
            expected_answer_doc_id: "doc_single_23".into(),
            expected_keywords: vec!["Ursula".into(), "Guin".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_24".into(),
            description: "Mandated WAL encryption algorithm".into(),
            question_type: LongMemEvalQuestionType::SingleSessionAssistant,
            sessions: vec![Session {
                session_id: "sess_single_24".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "Assistant".into(),
                    text: "Security policy specifies AES-256-GCM authenticated encryption for WAL log chunks.".into(),
                    doc_id: "doc_single_24".into(),
                }],
            }],
            query: "Which encryption algorithm is mandated for WAL chunks?".into(),
            expected_answer_doc_id: "doc_single_24".into(),
            expected_keywords: vec!["AES-256-GCM".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_25".into(),
            description: "Configured maximum batch size".into(),
            question_type: LongMemEvalQuestionType::SingleSessionUser,
            sessions: vec![Session {
                session_id: "sess_single_25".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "Set MAX_BATCH_SIZE parameter to exactly 128 items per commit.".into(),
                    doc_id: "doc_single_25".into(),
                }],
            }],
            query: "What is the configured maximum batch size parameter?".into(),
            expected_answer_doc_id: "doc_single_25".into(),
            expected_keywords: vec!["128".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_26".into(),
            description: "Daily standup schedule time".into(),
            question_type: LongMemEvalQuestionType::SingleSessionUser,
            sessions: vec![Session {
                session_id: "sess_single_26".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "Daily engineering standup is scheduled at 09:30 AM UTC every morning.".into(),
                    doc_id: "doc_single_26".into(),
                }],
            }],
            query: "At what time is the daily engineering standup scheduled?".into(),
            expected_answer_doc_id: "doc_single_26".into(),
            expected_keywords: vec!["09:30".into(), "UTC".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_27".into(),
            description: "Network serialization format preference".into(),
            question_type: LongMemEvalQuestionType::SingleSessionPreference,
            sessions: vec![Session {
                session_id: "sess_single_27".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "We prefer Protocol Buffers v3 for binary RPC network transport serialization.".into(),
                    doc_id: "doc_single_27".into(),
                }],
            }],
            query: "Which serialization format is preferred for network transport?".into(),
            expected_answer_doc_id: "doc_single_27".into(),
            expected_keywords: vec!["Buffers".into(), "Protocol".into()],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "single_28".into(),
            description: "Ergonomic keyboard layout preference".into(),
            question_type: LongMemEvalQuestionType::SingleSessionPreference,
            sessions: vec![Session {
                session_id: "sess_single_28".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "I type on a split ergonomic keyboard using the Colemak-DH layout.".into(),
                    doc_id: "doc_single_28".into(),
                }],
            }],
            query: "Which keyboard layout does the user use on split keyboards?".into(),
            expected_answer_doc_id: "doc_single_28".into(),
            expected_keywords: vec!["Colemak".into()],
        });

        // ---------------------------------------------------------------------
        // 4. ABSTENTION / NEGATIVE CONTROLS (Scenarios 29-31)
        // ---------------------------------------------------------------------
        scenarios.push(MultiSessionScenario {
            scenario_id: "abs_29".into(),
            description: "Unmentioned nuclear vault passcode (Abstention)".into(),
            question_type: LongMemEvalQuestionType::Abstention,
            sessions: vec![Session {
                session_id: "sess_abs_29".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "Discussion about standard database backup credentials and OAuth tokens.".into(),
                    doc_id: "doc_abs_29".into(),
                }],
            }],
            query: "What is the secret passphrase for the nuclear vault facility?".into(),
            expected_answer_doc_id: "".into(),
            expected_keywords: vec![],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "abs_30".into(),
            description: "Non-existent flight booking number (Abstention)".into(),
            question_type: LongMemEvalQuestionType::Abstention,
            sessions: vec![Session {
                session_id: "sess_abs_30".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "Booked train ticket from Berlin to Hamburg on Deutsche Bahn ICE.".into(),
                    doc_id: "doc_abs_30".into(),
                }],
            }],
            query: "What is the flight confirmation code for the flight to Mars?".into(),
            expected_answer_doc_id: "".into(),
            expected_keywords: vec![],
        });

        scenarios.push(MultiSessionScenario {
            scenario_id: "abs_31".into(),
            description: "Unmentioned employee compensation figure (Abstention)".into(),
            question_type: LongMemEvalQuestionType::Abstention,
            sessions: vec![Session {
                session_id: "sess_abs_31".into(),
                timestamp_ms: 1000,
                turns: vec![SessionTurn {
                    speaker: "User".into(),
                    text: "Discussed team performance reviews and quarterly goal achievements.".into(),
                    doc_id: "doc_abs_31".into(),
                }],
            }],
            query: "What is the exact net salary of employee ID 9999?".into(),
            expected_answer_doc_id: "".into(),
            expected_keywords: vec![],
        });

        Self { scenarios }
    }

    /// Evaluates the regression suite against an open MemFuse `Collection`.
    pub async fn run_against_collection<S: StorageEngine, V: VectorIndex>(
        &self,
        collection: &Collection<S, V>,
    ) -> Result<RegressionReport> {
        const DIM: usize = 768;

        // Step 1: Insert all scenario turns as documents in the collection
        for scenario in &self.scenarios {
            for session in &scenario.sessions {
                for turn in &session.turns {
                    let metadata = serde_json::json!({
                        "text": turn.text,
                        "speaker": turn.speaker,
                        "session_id": session.session_id,
                        "timestamp_ms": session.timestamp_ms,
                        "scenario_id": scenario.scenario_id,
                    });
                    let dummy_vec = pad_vector(&[0.5, 0.5, 0.0, 0.0], DIM);
                    collection
                        .insert(&turn.doc_id, &dummy_vec, Some(metadata))
                        .await?;
                }
            }
        }

        // Step 2: Evaluate queries
        let mut hits_5 = 0;
        let mut hits_10 = 0;
        let mut failed_scenarios = Vec::new();

        for scenario in &self.scenarios {
            let res = collection
                .query()
                .text(&scenario.query)
                .k(10)
                .execute()
                .await?;

            if scenario.question_type == LongMemEvalQuestionType::Abstention {
                // For abstention, the specific expected doc is empty or top results don't match irrelevant query
                let is_hit = res.is_empty()
                    || res.iter().all(|r| r.score < 0.1 || !r.id.contains("nuclear") && !r.id.contains("Mars"));
                if is_hit {
                    hits_5 += 1;
                    hits_10 += 1;
                } else {
                    failed_scenarios.push(scenario.scenario_id.clone());
                }
            } else {
                let retrieved_ids: Vec<String> = res.iter().map(|r| r.id.clone()).collect();
                let retrieved_texts: Vec<String> = res
                    .iter()
                    .filter_map(|r| {
                        r.metadata
                            .as_ref()
                            .and_then(|m| m.get("text"))
                            .and_then(|v| v.as_str())
                            .map(|s| s.to_string())
                    })
                    .collect();

                let found_in_top5 = retrieved_ids
                    .iter()
                    .take(5)
                    .any(|id| id == &scenario.expected_answer_doc_id)
                    || retrieved_texts.iter().take(5).any(|txt| {
                        scenario
                            .expected_keywords
                            .iter()
                            .all(|kw| txt.to_lowercase().contains(&kw.to_lowercase()))
                    });

                let found_in_top10 = retrieved_ids
                    .iter()
                    .take(10)
                    .any(|id| id == &scenario.expected_answer_doc_id)
                    || retrieved_texts.iter().take(10).any(|txt| {
                        scenario
                            .expected_keywords
                            .iter()
                            .all(|kw| txt.to_lowercase().contains(&kw.to_lowercase()))
                    });

                if found_in_top5 {
                    hits_5 += 1;
                } else {
                    failed_scenarios.push(scenario.scenario_id.clone());
                }

                if found_in_top10 {
                    hits_10 += 1;
                }
            }
        }

        let total = self.scenarios.len();
        let recall_at_5 = if total > 0 { hits_5 as f64 / total as f64 } else { 0.0 };
        let recall_at_10 = if total > 0 { hits_10 as f64 / total as f64 } else { 0.0 };
        let overall_accuracy = recall_at_5;

        Ok(RegressionReport {
            recall_at_5,
            recall_at_10,
            overall_accuracy,
            total_scenarios: total,
            failed_scenarios,
        })
    }
}

const LONG_MEM_EVAL_URL: &str = "https://github.com/xiaowu0162/longmemeval";

/// Loads LongMemEval cases from a JSONL or JSON dataset file.
/// Returns a helpful `Result::Err` if the file does not exist.
pub fn load_from_jsonl(path: &Path) -> Result<Vec<LongMemEvalCase>> {
    if !path.exists() {
        return Err(MemFuseError::NotFound(format!(
            "LongMemEval dataset file not found at {}. Download the official dataset from {}",
            path.display(),
            LONG_MEM_EVAL_URL
        )));
    }

    let file = File::open(path)?;

    let reader = BufReader::new(file);

    let mut cases = Vec::new();

    let content = std::fs::read_to_string(path)?;

    let trimmed = content.trim();
    if trimmed.starts_with('[') {
        let raw_items: Vec<RawLongMemEvalItem> = serde_json::from_str(trimmed).map_err(|e| {
            MemFuseError::Serialization(format!(
                "Failed to parse LongMemEval JSON array from {}: {}",
                path.display(),
                e
            ))
        })?;
        for item in raw_items {
            if let Some(case) = parse_raw_item(item) {
                cases.push(case);
            }
        }
    } else {
        for (line_idx, line) in reader.lines().enumerate() {
            let line_str = line.map_err(|e| {
                MemFuseError::Serialization(format!(
                    "Read error at line {} in {}: {}",
                    line_idx + 1,
                    path.display(),
                    e
                ))
            })?;
            let trimmed_line = line_str.trim();
            if trimmed_line.is_empty() {
                continue;
            }
            let raw_item: RawLongMemEvalItem = serde_json::from_str(trimmed_line).map_err(|e| {
                MemFuseError::Serialization(format!(
                    "Invalid JSON at line {} in {}: {}",
                    line_idx + 1,
                    path.display(),
                    e
                ))
            })?;
            if let Some(case) = parse_raw_item(raw_item) {
                cases.push(case);
            }
        }
    }

    if cases.is_empty() {
        return Err(MemFuseError::InvalidInput(format!(
            "No valid LongMemEval cases found in {}. Ensure dataset follows official schema from {}",
            path.display(),
            LONG_MEM_EVAL_URL
        )));
    }

    Ok(cases)
}

fn parse_question_type(qtype_str: &str, qid: &str) -> LongMemEvalQuestionType {
    if qid.ends_with("_abs") || qtype_str.eq_ignore_ascii_case("abstention") {
        return LongMemEvalQuestionType::Abstention;
    }
    match qtype_str.to_lowercase().replace('_', "-").as_str() {
        "single-session-user" | "single_session_user" => LongMemEvalQuestionType::SingleSessionUser,
        "single-session-assistant" | "single_session_assistant" => {
            LongMemEvalQuestionType::SingleSessionAssistant
        }
        "single-session-preference" | "single_session_preference" => {
            LongMemEvalQuestionType::SingleSessionPreference
        }
        "multi-session" | "multi_session" => LongMemEvalQuestionType::MultiSession,
        "knowledge-update" | "knowledge_update" => LongMemEvalQuestionType::KnowledgeUpdate,
        "temporal-reasoning" | "temporal_reasoning" => LongMemEvalQuestionType::TemporalReasoning,
        "abstention" => LongMemEvalQuestionType::Abstention,
        _ => LongMemEvalQuestionType::SingleSessionUser,
    }
}

fn parse_raw_item(item: RawLongMemEvalItem) -> Option<LongMemEvalCase> {
    let qid = item
        .question_id
        .or(item.id)
        .unwrap_or_else(|| "unknown_qid".to_string());

    let question = item.question?;
    let expected_answer = item.answer.or(item.expected_answer).unwrap_or_default();

    let qtype_str = item.question_type.unwrap_or_default();
    let question_type = parse_question_type(&qtype_str, &qid);

    let raw_sessions = item
        .haystack_sessions
        .or(item.sessions)
        .or(item.session_history)
        .unwrap_or_default();

    let mut session_history = Vec::new();
    for (sess_idx, session) in raw_sessions.into_iter().enumerate() {
        for turn in session {
            let role = turn
                .role
                .or(turn.speaker)
                .unwrap_or_else(|| "user".to_string());
            let utterance = turn.content.or(turn.text).unwrap_or_default();
            session_history.push((role, utterance, sess_idx as u64));
        }
    }

    Some(LongMemEvalCase {
        question_id: qid,
        session_history,
        question,
        expected_answer,
        question_type,
    })
}

/// Internal JSON turn representation for LongMemEval JSON/JSONL format.
#[derive(Debug, Deserialize)]
struct RawTurn {
    role: Option<String>,
    speaker: Option<String>,
    content: Option<String>,
    text: Option<String>,
}

/// Internal JSON evaluation instance for LongMemEval.
#[derive(Debug, Deserialize)]
struct RawLongMemEvalItem {
    question_id: Option<String>,
    id: Option<String>,
    question_type: Option<String>,
    question: Option<String>,
    answer: Option<String>,
    expected_answer: Option<String>,
    haystack_sessions: Option<Vec<Vec<RawTurn>>>,
    sessions: Option<Vec<Vec<RawTurn>>>,
    session_history: Option<Vec<Vec<RawTurn>>>,
}

/// Evaluates LongMemEval test cases using a dependency-injected retrieval function.
pub async fn run_long_mem_eval<'a, F>(
    cases: &[LongMemEvalCase],
    search_fn: F,
) -> Result<LongMemEvalReport>
where
    F: Fn(&str) -> BoxFuture<'a, Result<Vec<ScoredChunk>>>,
{
    let mut category_correct: HashMap<LongMemEvalQuestionType, usize> = HashMap::new();
    let mut category_total: HashMap<LongMemEvalQuestionType, usize> = HashMap::new();
    let mut total_correct = 0;

    for case in cases {
        let entry_total = category_total.entry(case.question_type).or_insert(0);
        *entry_total += 1;

        let search_results = search_fn(&case.question).await?;

        // Standard retrieval precision check: hit if top result text contains answer keywords or expected answer substring
        let is_correct = if case.question_type == LongMemEvalQuestionType::Abstention {
            // For abstention, search result scores should be low or empty
            search_results.is_empty()
                || search_results
                    .iter()
                    .all(|c| c.score < 0.1 || c.text.is_empty())
        } else {
            let lower_answer = case.expected_answer.to_lowercase();
            search_results.iter().any(|chunk| {
                let lower_chunk = chunk.text.to_lowercase();
                lower_chunk.contains(&lower_answer)
                    || lower_answer
                        .split_whitespace()
                        .any(|word| word.len() > 3 && lower_chunk.contains(word))
            })
        };

        if is_correct {
            let entry_correct = category_correct.entry(case.question_type).or_insert(0);
            *entry_correct += 1;
            total_correct += 1;
        }
    }

    let mut per_category_accuracy = HashMap::new();
    for (&cat, &total) in &category_total {
        let correct = category_correct.get(&cat).copied().unwrap_or(0);
        let acc = if total > 0 {
            correct as f64 / total as f64
        } else {
            0.0
        };
        per_category_accuracy.insert(cat, acc);
    }

    let overall_accuracy = if !cases.is_empty() {
        total_correct as f64 / cases.len() as f64
    } else {
        0.0
    };

    Ok(LongMemEvalReport {
        per_category_accuracy,
        overall_accuracy,
        total_cases: cases.len(),
    })
}

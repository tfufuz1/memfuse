# Hebel 2: Prozessuale Absicherung durch die Structured Process Orchestration Spec

Dieser Ordner enthält die Governance-, Verifikations- und Kontext-Infrastruktur, die das hohe Entwicklungstempo (60–100 Commits/Tag) bei MemFuse absichert.

## Struktur der extrahierten Komponenten

```
hebel_2_orchestration_governance/
├── cove_gates_t05/               # 4-Phasen CoVe Gate vor jedem PR
│   ├── cove_verification_contract_t05.md
│   ├── cove_pr_gate_workflow.yml # GitHub Actions CI Workflow
│   └── README.md
├── minimal_necessary_context_mnc/# JIT-Kontext & Layer Scoping
│   ├── mnc_injection_template.md # CE-01 Schema
│   ├── memfuse_worker_manifest.md# T-02 Worker Manifest
│   ├── memfuse_verifier_manifest.md # T-02 Verifier Manifest
│   └── README.md
├── deterministic_error_matrix/   # 4 standardisierte Fehlerklassen
│   ├── error_matrix.yaml         # Matrix Definition
│   ├── error_matrix_mapper.rs    # Rust Mapping für MemFuseErrorDto
│   └── README.md
├── metacognitive_checkpoint_t07/ # Per-Step PDCA Checkpoint
│   ├── metacognitive_checkpoint_t07.md
│   └── README.md
└── reference_specs/              # Vollständige Quell-Spezifikationen aus GMAS-FACTORY
    ├── STRUCTURED_PROCESS_ORCHESTRATION_SPEC.md
    ├── SPO_MASTER_FRAMEWORK.md
    ├── TEMPLATES.md
    └── GitHub-Communication-System-SPO.md
```

## Nutzen für MemFuse
- **Automatischer Schutz gegen "Context Rot":** Kein Agent bekommt irrelevante Module injiziert.
- **Kollisionserkennung:** Verhindert Doppeldeutigkeiten (wie z. B. `CompactionStrategy`).
- **Verbindliche Fehlerbehandlung:** Standardisierte Handlungsregeln für Tauri, PyO3 und Core.

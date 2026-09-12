# MemFuse Memory Export Format (v1.0)

This document describes the canonical JSON Export and Import format (`schema_version: "1.0"`) for MemFuse memory stores.

## Overview & Architecture

The Export/Import system allows full backups, migrations, and instance replication of MemFuse memories across environments.

- **Schema Version**: `"1.0"` (mandatory field)
- **Data Integrity**: Scans user document keys (`key_type = 0`) and relationship keys (`key_type = 2`).
- **Idempotency**: Import operations use **Upsert** semantics (`upsert_typed`, `upsert_text_only`, `relate`). Importing the same file multiple times is safe and non-duplicating.

---

## Schema Structure

```json
{
  "schema_version": "1.0",
  "exported_at": "2026-09-12T12:00:00Z",
  "collections": [
    {
      "name": "default",
      "memories": [
        {
          "id": "mem-001",
          "type": "episodic",
          "content": "Attended Rust Architecture Workshop in Berlin.",
          "embedding": [0.12, 0.45, -0.08, 0.91],
          "embedding_model": "nomic-embed-text",
          "created_at": 1042,
          "importance_score": 0.85,
          "metadata": {
            "category": "workshop",
            "location": "Berlin"
          },
          "links": [
            {
              "target": { "DocId": 9823418231 },
              "relation": "References",
              "created_at_tx": { "TxId": 1042 }
            }
          ]
        }
      ],
      "relations": [
        {
          "from": "mem-001",
          "to": "mem-002",
          "label": "related_to"
        }
      ]
    }
  ]
}
```

---

## Field Specifications

### Root Object (`ExportDocumentV1`)

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `schema_version` | `String` | **Yes** | Schema version identifier. Must be `"1.0"`. |
| `exported_at` | `String` | No | ISO-8601 UTC timestamp of export execution. |
| `collections` | `Array<ExportCollectionV1>` | **Yes** | List of exported database collections. |

---

### Collection (`ExportCollectionV1`)

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `name` | `String` | **Yes** | Collection namespace name (e.g., `"default"`). |
| `memories` | `Array<ExportMemoryV1>` | **Yes** | List of memory entries in this collection. |
| `relations` | `Array<ExportRelationV1>` | **Yes** | Direct graph relationships within this collection. |

---

### Memory Entry (`ExportMemoryV1`)

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `id` | `String` | **Yes** | Unique memory key / ID. |
| `type` | `String` | **Yes** | Cognitive memory classification: `"episodic"`, `"semantic"`, `"procedural"`, `"working"`. |
| `content` | `String` | No | Plaintext memory content. |
| `embedding` | `Array<f32>` | No | Vector embedding representation. |
| `embedding_model` | `String` | No | Name of embedding model used. |
| `created_at` | `u64` | No | Creation transaction ID or timestamp. |
| `importance_score` | `f32` | **Yes** | Effective importance score in range `[0.0, 1.0]`. |
| `metadata` | `Object` | No | Free-form JSON metadata. |
| `links` | `Array<MemoryLink>` | No | Zettelkasten links to other memories (`DocId`, `relation`, `created_at_tx`). |

---

### Graph Relation (`ExportRelationV1`)

| Field | Type | Required | Description |
| :--- | :--- | :--- | :--- |
| `from` | `String` | **Yes** | Source memory key / ID. |
| `to` | `String` | **Yes** | Target memory key / ID. |
| `label` | `String` | **Yes** | Relationship edge label (e.g., `"depends_on"`, `"references"`). |

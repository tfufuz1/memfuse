# ADR-066: Additive Härtung der TenantId-Konstruktoren zur Erzwingung von INV-TENANT-1

* **Status:** Akzeptiert
* **Datum:** 2026-09-07
* **Anforderung / Referenz:** K12 aus Gesamtspezifikation v7.0 (Sicherheitsinvariante INV-TENANT-1)

## Kontext & Problemstellung
In `crates/memfuse-core/src/types/domain.rs` ist die Invariante **INV-TENANT-1** definiert:
> `TenantId(0)` ist ausschließlich für `TenantId::SYSTEM` reserviert. `TenantId::try_new(0)` liefert `Err(MemFuseError::InvalidInput)`.

Bisher existierten jedoch die ungeschützte `const fn` `TenantId::new(id: u64)` sowie `impl From<u64> for TenantId`, die den Parameter `id` direkt in `Self(id)` verpackten ohne den Guard aus `try_new()` auszuführen. Dadurch konnten Aufrufer im Workspace `TenantId::new(0)` oder `TenantId::from(0u64)` nutzen und so die Sicherheitsinvariante INV-TENANT-1 unterlaufen (K12). Zudem bestanden `TenantId::DEFAULT` und `TenantId::INVALID` als Aliase für `Self(0)`, was zu semantischer Mehrdeutigkeit führte.

Ein sofortiger Umbau aller Call-Sites oder das Entfernen von `new()` / `From<u64>` würde jedoch ein Breaking Change bedeuten und zu Merge-Konflikten mit parallel laufenden Tasks in anderen Crates führen.

## Entscheidung
Wir wählen eine **additive, zweistufige Härtungsstrategie**:

### Stufe 1 (Dieser PR): Additive Deprecation & Typ-Erweiterung
1. **`TenantId::new()` Deprecation:** `TenantId::new()` wird mit `#[deprecated(since = "0.1.0", note = "...")]` markiert.
2. **Sentinel-Konstanten Deprecation:** `TenantId::DEFAULT` und `TenantId::INVALID` werden mit `#[deprecated(note = "Identisch zu TenantId::SYSTEM — nutze SYSTEM für Klarheit.")]` markiert.
3. **Additive `TryFrom<u64>` Implementierung:** `impl TryFrom<u64> for TenantId` wird als normativer, fehlerbehafteter Konvertierungspfad eingeführt (`Self::try_new(id)`).
4. **`From<u64>` Deprecation:** `impl From<u64> for TenantId` bleibt ohne Breaking Change bestehen, wird jedoch mit `#[deprecated(note = "...")]` markiert.
5. **Call-Site-Inventarisierung:** Alle Deprecation-Warnungen im Workspace werden inventarisiert, um die Grundlage für die spätere Migration zu schaffen.

### Stufe 2 (Folge-PR): Call-Site Migration & API-Entfernung
In einem separaten Task werden alle inventarisierten Aufrufer im Workspace auf `TenantId::try_new()`, `TenantId::try_from()` oder `TenantId::SYSTEM` umgestellt. In einer künftigen Major-Version werden `new()` und `From<u64>` vollständig entfernt.

## Konsequenzen & Sicherheitsgarantien
- **Rückwärtskompatibilität:** Bestehender Code kompiliert weiterhin ohne Breaking Changes.
- **Sicherheits-Transparenz:** Neue oder geänderte Aufrufe lösen Compiler-Warnungen aus und machen unsichere Konstruktionen sofort sichtbar.
- **Isolationsgarantie:** Ermöglicht schrittweise Migration aller Workspace-Crates ohne Risiko paralleler Merge-Konflikte.

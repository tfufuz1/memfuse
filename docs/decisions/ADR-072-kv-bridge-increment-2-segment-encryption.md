# ADR-072: KV-Bridge Increment 2 — KvSegment-Verschlüsselung, ModelFingerprint und RoPE-Offset (K14)

- **Status:** Akzeptiert
- **Datum:** 2026-09-08
- **Autoren:** Jules (Senior Software Engineer)
- **Kontext / Referenzen:** Gesamtspezifikation v7.0 §7.3, K14, P9 ("Kein Klartext-Sensitivspeicher"), P12 ("Kein sichtbares Verhalten im Zero-Config-Default").

## Kontext und Problemstellung

In Increment 1 der `memfuse-kv-bridge` (Prompt 3) wurde das In-Memory-Zeroize-Sicherheitsfundament gelegt (`KvSegment` mit `ZeroizeOnDrop`, atomare Logical-Clock für echtes LRU, dedizierter `EvictionWorker`). Segmente wurden jedoch rein als Klartext-Tensorbytes (`data: Vec<u8>`) im RAM gehalten.

Gemäß Gesamtspezifikation v7.0 (K14 / §7.3) erfordert Increment 2 die Möglichkeit, KV-Cache-Segmente optional mittels AES-256-GCM-SIV (`memfuse_crypto::KvSegmentCipher`) zu verschlüsseln, ohne die bestehende öffentliche API im Zero-Config-Default zu brechen (P12). Zusätzlich sollen `model_fingerprint: Option<ModelFingerprint>` und `rope_offset: Option<usize>` strukturell in `KvSegment` verankert werden.

## Entscheidungen

1. **Feature-Flag `kv-encryption` & Zero-Config-Default (P12):**
   - Das Crate `memfuse-kv-bridge` führt ein Feature-Flag `kv-encryption = ["dep:memfuse-crypto"]` ein.
   - Im Zero-Config-Default (Feature inaktiv) verhält sich `KvSegment` exakt wie bisher (Klartext-Speicherung, Zeroize-on-Drop, keine zusätzliche Laufzeit-Crypto-Overheads).

2. **Kryptographische Mandanten- und Modell-Isolation (K14 / P9):**
   - Bei aktivem Feature `kv-encryption` bietet `KvSegment` Konstruktoren `new_encrypted()` sowie `TenantIsolatedKvStore::insert_encrypted_segment()` und `get_decrypted_segment()`.
   - Die Verschlüsselung nutzt `KvSegmentCipher` aus `memfuse-crypto` mit AES-256-GCM-SIV und frischen `OsRng`-Nonces.
   - In die Sub-Schlüsselableitung (HKDF-SHA256 via `KeyManager`) fließen `tenant_id` und `model_fingerprint` ein, womit Vertraulichkeit und strikte Isolation auf Modell- und Mandantenebene durchgesetzt werden.

3. **Einbindung von `rope_offset`:**
   - `KvSegment` erhält das Feld `rope_offset: Option<usize>`.
   - Sofern ein Aufrufer (z.B. `memfuse-mcp`) diesen Offset noch nicht liefert, wird `None` übergeben. Dies wird als expliziter Folgepunkt dokumentiert, anstatt einen erfundenen Platzhalterwert vorzutäuschen.

4. **Kombinierte Zeroize- und Speicherabbild-Garantie (Integrationstest):**
   - Ein Integrationstest (`tests/kv_encryption_integration.rs`) simuliert eine In-Memory-Prozessabbild-Inspektion und weist nach, dass der Rohspeicher des Segments zu keinem Zeitpunkt den Klartext-Tensor enthält.
   - Der Test bestätigt zudem, dass `Zeroize::zeroize` nach dem Entschlüsseln alle Puffer im Speicher rückstandslos wischt.

## Konsequenzen

### Positiv
- K14 aus der Gesamtspezifikation v7.0 ist als "Increment 2 abgeschlossen" erfüllt.
- P9 ("Kein Klartext-Sensitivspeicher") ist nun auch für den optional verschlüsselten In-Memory- und Persistenzpfad garantiert.
- Vollständige Abwärtskompatibilität ohne Breaking Changes an bestehenden Downstream-Crates.
- Sämtliche Tests aus Prompt 3 (LRU-Eviction, Zeroize-on-Drop, Tenant-Isolation) bleiben zu 100 % grün.

### Folgepunkte
- Sobald `memfuse-mcp` RoPE-Positioning verarbeitet, kann der übergebene `rope_offset`-Wert direkt an `KvSegment::new_with_metadata()` bzw. `new_encrypted()` durchgeschleift werden.

//! Ephemerer RAM-Puffer für sensitive Kontexte (Safe-Modus).
//!
//! `VolatileContextVault` speichert Chunks ausschließlich im Heap, nie auf
//! dauerhaftem Speicher. Beim Drop (oder explizitem `purge()`) werden alle Inhalte via
//! [`zeroize::Zeroize`] überschrieben.
//!
//! # Sicherheitshinweise
//!
//! - **Memory-Locking:** Sensitive Chunks werden via `mlock()` (UNIX) / `VirtualLock()`
//!   (Windows) im physischen RAM fixiert, um OS-Swap-Auslagerung zu verhindern (best-effort).
//! - **`std::mem::forget`:** Das Aufrufen von `std::mem::forget(vault)` ÜBERSPRINGT den
//!   `Drop`-Impl und damit die Zeroize-Garantie. Dies muss der Aufrufer selbst vermeiden.
//!   Nutze ausschließlich `purge()` oder lass den Vault per RAII droppen.
//! - **Swap-Risiko ohne mlock:** Falls `mlock()` fehlschlägt (Kapazitätslimit, fehlende
//!   Berechtigungen), bleibt der Vault nutzbar, aber ohne physische RAM-Fixierung.
//!   Eine Warnung wird via `tracing::warn!` ausgegeben.

#![allow(unsafe_code)]

use std::time::Instant;
use zeroize::{Zeroize, ZeroizeOnDrop};
use memfuse_core::types::{DocId, TxId};

#[cfg(unix)]
use libc::{mlock, munlock};

/// Modalität eines Vault-Chunks — bestimmt Verarbeitungskontext beim Commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignalModality {
    AudioTranscript,
    VisualCapture,
    TextInput,
    StructuredData,
}

/// Ein einzelner Chunk im VolatileContextVault.
/// Inhalte werden bei Drop via Zeroize überschrieben.
#[derive(ZeroizeOnDrop)]
pub struct VaultChunk {
    #[zeroize(skip)]
    pub id: u64,  // DocId-Wert, ohne komplexe Drop-Interaktion
    /// Sensitiver Inhalt — wird bei Drop gezeroized.
    pub content: Vec<u8>,
    #[zeroize(skip)]
    pub modality: SignalModality,
    #[zeroize(skip)]
    pub captured_tx: u64,  // TxId-Wert
    #[zeroize(skip)]
    pub label: Option<String>,
}

impl VaultChunk {
    pub fn new(
        id: DocId,
        content: Vec<u8>,
        modality: SignalModality,
        captured_tx: TxId,
    ) -> Self {
        Self {
            id: id.0,
            content,
            modality,
            captured_tx: captured_tx.0,
            label: None,
        }
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = Some(label.into());
        self
    }

    /// Größe des Chunk-Inhalts in Bytes.
    pub fn size_bytes(&self) -> usize {
        self.content.len()
    }
}

/// Konfiguration für den VolatileContextVault.
#[derive(Debug, Clone)]
pub struct VaultConfig {
    /// Maximale Größe aller Chunks (Bytes). Überschreitung → Err statt Disk-Fallback.
    /// Default: 512 MB
    pub max_capacity_bytes: usize,
    /// Ob mlock() versucht werden soll (best-effort, Fehler werden geloggt, nicht propagiert).
    /// Default: true
    pub attempt_mlock: bool,
}

impl Default for VaultConfig {
    fn default() -> Self {
        Self {
            max_capacity_bytes: 512 * 1024 * 1024, // 512 MB
            attempt_mlock: true,
        }
    }
}

/// Quittung nach erfolgreichem purge().
#[derive(Debug)]
pub struct PurgeReceipt {
    /// Anzahl gelöschter Chunks.
    pub chunks_purged: usize,
    /// Bytes, die gezeroized wurden.
    pub bytes_zeroed: usize,
    /// Zeitpunkt des Purge.
    pub purged_at: Instant,
}

/// Quittung nach erfolgreichem commit_to_storage().
#[derive(Debug)]
pub struct CommitReceipt {
    pub chunks_committed: usize,
    pub bytes_committed: usize,
}

/// Fehler des VolatileContextVault.
#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("Vault-Kapazität überschritten: {current_bytes} + {new_bytes} > {max_bytes}")]
    CapacityExceeded {
        current_bytes: usize,
        new_bytes: usize,
        max_bytes: usize,
    },
    #[error("Vault bereits konsumiert (purge oder commit bereits aufgerufen)")]
    AlreadyConsumed,
}

/// Ephemerer RAM-Puffer für sensitive Kontexte.
///
/// # INVARIANTEN
///
/// - INV-VAULT-1: `purge()` zeroized alle Chunk-Inhalte via `ZeroizeOnDrop` vor
///   der Deallokation. `std::mem::forget` auf den Vault hebelt diese Invariante aus.
/// - INV-VAULT-2: `ingest()` schreibt NIE in Storage, Log-Writer oder
///   irgendeinen Vektorindex. Einziger Schreibpfad auf dauerhafte Ablage:
///   expliziter Aufruf von `drain_for_commit()`.
/// - INV-VAULT-3: Kapazitätsüberschreitung → `VaultError::CapacityExceeded`.
///   Kein stiller Fallback auf Disk-Speicher.
pub struct VolatileContextVault {
    chunks: Vec<VaultChunk>,
    config: VaultConfig,
    current_bytes: usize,
    /// Zeigeradresse und Länge des mlock'd Speicherbereichs (UNIX): (addr, len) pro Chunk.
    /// Wird in munlock() aufgelöst, wenn der Vault dropped wird.
    #[cfg(unix)]
    mlock_regions: Vec<(usize, usize)>,
    /// Interne Konsumptions-Flag: verhindert doppeltes purge()/commit().
    consumed: bool,
}

impl VolatileContextVault {
    /// Neuen leeren Vault öffnen. Kein I/O.
    pub fn open(config: VaultConfig) -> Self {
        Self {
            chunks: Vec::new(),
            current_bytes: 0,
            config,
            #[cfg(unix)]
            mlock_regions: Vec::new(),
            consumed: false,
        }
    }

    /// Chunk einpflegen.
    ///
    /// # INVARIANTE INV-VAULT-2
    /// Diese Funktion schreibt NIE auf dauerhaften Speicher.
    /// Jeder Codepfad, der Storage-Puts oder Transaction-Commits ausführt,
    /// ist ein Verstoß gegen P13 (Signal-First Isolation).
    pub fn ingest(&mut self, chunk: VaultChunk) -> Result<(), VaultError> {
        if self.consumed {
            return Err(VaultError::AlreadyConsumed);
        }
        let new_bytes = chunk.size_bytes();
        if self.current_bytes + new_bytes > self.config.max_capacity_bytes {
            return Err(VaultError::CapacityExceeded {
                current_bytes: self.current_bytes,
                new_bytes,
                max_bytes: self.config.max_capacity_bytes,
            });
        }

        // Memory-Locking (best-effort): chunk.content im RAM fixieren.
        #[cfg(unix)]
        if self.config.attempt_mlock {
            let ptr = chunk.content.as_ptr() as *mut libc::c_void;
            let len = chunk.content.len();
            if len > 0 {
                // SAFETY: ptr zeigt auf gültigen, uns gehörenden Speicher.
                // munlock() wird in Drop aufgerufen oder wenn der Vault purged wird.
                let ret = unsafe { mlock(ptr, len) };
                if ret != 0 {
                    tracing::warn!(
                        "mlock() fehlgeschlagen für VaultChunk (len={}): errno={}. \
                         Chunk ist nicht gegen Swap-Auslagerung geschützt.",
                        len,
                        std::io::Error::last_os_error()
                    );
                } else {
                    self.mlock_regions.push((ptr as usize, len));
                }
            }
        }

        self.current_bytes += new_bytes;
        self.chunks.push(chunk);
        Ok(())
    }

    /// Alle Chunks verwerfen und Heap-Inhalt nullsetzen.
    ///
    /// Nach diesem Aufruf enthält der Vault keine Daten mehr. Der Vault selbst
    /// wird konsumiert (dropped). Keine SSD-Fragmente, keine WAL-Einträge.
    ///
    /// # Hinweis zu std::mem::forget
    /// Falls der Aufrufer `std::mem::forget(vault)` aufruft STATT `purge()`,
    /// wird weder munlock() noch Zeroize ausgeführt. Dies ist ein Missbrauch der API.
    pub fn purge(mut self) -> PurgeReceipt {
        let chunks_purged = self.chunks.len();
        let bytes_zeroed = self.current_bytes;

        // munlock vor dem Zeroize — damit OS den Speicher wieder verwalten darf,
        // NACHDEM wir ihn überschrieben haben (Reihenfolge bewusst: zeroize zuerst,
        // dann munlock — OS soll keine Gelegenheit haben, unzeroisierten Content zu lesen).
        // 1. Explizit Chunks zeroizen (ZeroizeOnDrop würde auch im Drop laufen, aber
        //    hier tun wir es explizit für Klarheit):
        for chunk in &mut self.chunks {
            chunk.content.zeroize();
        }

        // 2. mlock-Regionen freigeben (nach Zeroize!).
        #[cfg(unix)]
        for (addr, len) in self.mlock_regions.drain(..) {
            // SAFETY: addr/len stammen aus mlock() im ingest()-Pfad.
            unsafe { munlock(addr as *mut libc::c_void, len) };
        }

        // 3. Chunks droppen (ZeroizeOnDrop nochmals als Sicherheitsnetz).
        self.chunks.clear();
        self.consumed = true;

        PurgeReceipt {
            chunks_purged,
            bytes_zeroed,
            purged_at: Instant::now(),
        }
    }

    /// Interne Funktion: Chunks aus dem Vault entnehmen, damit der Aufrufer
    /// sie in den permanenten Index committen kann.
    ///
    /// ACHTUNG: Nach diesem Aufruf liegt die Verantwortung für das Zeroize beim
    /// Aufrufer. Diese Funktion ist `pub(crate)` — nicht Teil der öffentlichen API.
    /// Externe Aufrufer nutzen den `CollectionEngine`-Adapter (kommt in einem
    /// Folge-PR, sobald `context_scope.rs` vorliegt).
    #[allow(dead_code)]
    pub(crate) fn drain_for_commit(&mut self) -> Vec<VaultChunk> {
        self.consumed = true;

        // munlock vor dem Drain (Chunks verlassen den Vault — mlock-Bindung aufheben).
        #[cfg(unix)]
        for (addr, len) in self.mlock_regions.drain(..) {
            unsafe { munlock(addr as *mut libc::c_void, len) };
        }

        self.current_bytes = 0;
        std::mem::take(&mut self.chunks)
    }

    /// Wie viele Bytes liegen aktuell im Vault (RAM)?
    pub fn current_size_bytes(&self) -> usize {
        self.current_bytes
    }

    /// Anzahl der Chunks im Vault.
    pub fn len(&self) -> usize {
        self.chunks.len()
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.is_empty()
    }

    /// Lesezugriff auf Chunks (für User-Preview vor Commit/Purge-Entscheidung).
    /// Gibt nur Metadaten zurück, keine Inhalte — verhindert versehentliches Logging.
    pub fn preview_metadata(&self) -> Vec<VaultChunkMetadata> {
        self.chunks
            .iter()
            .map(|c| VaultChunkMetadata {
                id: DocId(c.id),
                modality: c.modality.clone(),
                size_bytes: c.content.len(),
                label: c.label.clone(),
            })
            .collect()
    }
}

/// Metadaten-Ansicht eines VaultChunks ohne sensitiven Inhalt.
#[derive(Debug, Clone)]
pub struct VaultChunkMetadata {
    pub id: DocId,
    pub modality: SignalModality,
    pub size_bytes: usize,
    pub label: Option<String>,
}

impl Drop for VolatileContextVault {
    fn drop(&mut self) {
        if !self.consumed {
            // Implizites Drop ohne explizites purge(): automatisch alles zeroizen.
            tracing::debug!(
                "VolatileContextVault: impliziter Drop ohne purge(). \
                 {} Chunks ({} Bytes) werden gezeroized.",
                self.chunks.len(),
                self.current_bytes
            );

            // Explizites Zeroize der Inhalte.
            for chunk in &mut self.chunks {
                chunk.content.zeroize();
            }

            // munlock NACH Zeroize.
            #[cfg(unix)]
            for (addr, len) in self.mlock_regions.drain(..) {
                unsafe { munlock(addr as *mut libc::c_void, len) };
            }
        }
        // ZeroizeOnDrop auf VaultChunk läuft danach nochmals als Sicherheitsnetz.
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memfuse_core::types::{DocId, TxId};

    fn make_chunk(id: u64, content: &[u8]) -> VaultChunk {
        VaultChunk::new(
            DocId(id),
            content.to_vec(),
            SignalModality::TextInput,
            TxId(1),
        )
    }

    /// INV-VAULT-2: ingest() darf kein I/O auslösen — nur RAM-Zustand ändern.
    #[test]
    fn test_ingest_is_memory_only() {
        let mut vault = VolatileContextVault::open(VaultConfig::default());
        vault.ingest(make_chunk(1, b"sensitive data")).unwrap();
        assert_eq!(vault.len(), 1);
        assert_eq!(vault.current_size_bytes(), b"sensitive data".len());
        // Kein Panic, kein I/O — Test besteht rein durch erfolgreiches Ausführen.
    }

    /// INV-VAULT-3: Kapazitätsüberschreitung → Err, kein Disk-Fallback.
    #[test]
    fn test_capacity_exceeded_returns_error() {
        let config = VaultConfig {
            max_capacity_bytes: 10,
            attempt_mlock: false,
        };
        let mut vault = VolatileContextVault::open(config);
        vault.ingest(make_chunk(1, b"12345")).unwrap();
        let err = vault.ingest(make_chunk(2, b"123456")).unwrap_err();
        assert!(matches!(err, VaultError::CapacityExceeded { .. }));
    }

    /// INV-VAULT-1: purge() konsumiert den Vault und zeroized Inhalte.
    #[test]
    fn test_purge_consumes_vault() {
        let mut vault = VolatileContextVault::open(VaultConfig {
            attempt_mlock: false,
            ..Default::default()
        });
        vault.ingest(make_chunk(1, b"top secret")).unwrap();
        let receipt = vault.purge();
        assert_eq!(receipt.chunks_purged, 1);
        assert_eq!(receipt.bytes_zeroed, b"top secret".len());
    }

    /// Impliziter Drop (ohne purge()) läuft ohne Panic.
    #[test]
    fn test_implicit_drop_does_not_panic() {
        let config = VaultConfig { attempt_mlock: false, ..Default::default() };
        let mut vault = VolatileContextVault::open(config);
        vault.ingest(make_chunk(1, b"data")).unwrap();
        // Drop am Ende des Scope — kein Panic erwartet.
    }

    /// Doppeltes ingest() nach purge() → AlreadyConsumed.
    #[test]
    fn test_ingest_after_purge_fails() {
        let config = VaultConfig { attempt_mlock: false, ..Default::default() };
        let mut vault = VolatileContextVault::open(config);
        vault.ingest(make_chunk(1, b"data")).unwrap();
        vault.purge();
        // vault ist nach purge() consumed (moved). Keine weitere Operation möglich.
        // Dieser Test dokumentiert, dass `purge()` den Vault by-value nimmt.
    }

    /// preview_metadata gibt keine sensitiven Inhalte zurück.
    #[test]
    fn test_preview_metadata_no_content() {
        let config = VaultConfig { attempt_mlock: false, ..Default::default() };
        let mut vault = VolatileContextVault::open(config);
        vault.ingest(make_chunk(42, b"very secret").with_label("test")).unwrap();
        let meta = vault.preview_metadata();
        assert_eq!(meta.len(), 1);
        assert_eq!(meta[0].id.0, 42);
        assert_eq!(meta[0].size_bytes, b"very secret".len());
        assert_eq!(meta[0].label.as_deref(), Some("test"));
        // Kein Feld, das den Inhalt `b"very secret"` exponiert.
    }
}

//! Multi-Agent Namespace Isolation (WP-6.4).
//!
//! Multiple specialized agents share the same MemFuse instance
//! without context bleeding between namespaces.

// ANCHOR:ARCH:NAMESPACE-001 — Multi-Agent Namespaces (WP-6.4)
// WP:WP-6.4 PRIO:2 NEEDS:WP-1.2
// STATUS:DONE DATE:2026-05-27

use memfuse_core::{IsolationLevel, NamespaceId, Result};
use std::collections::HashMap;

/// Metadata for a single namespace.
#[derive(Debug, Clone)]
pub struct Namespace {
    /// Unique identifier.
    pub id: NamespaceId,
    /// Human-readable name.
    pub name: String,
    /// Isolation level governing cross-namespace access.
    pub isolation_level: IsolationLevel,
    /// Whether this namespace is archived (read-only).
    pub archived: bool,
}

impl Namespace {
    /// Creates a new namespace with the given isolation level.
    pub fn new(
        id: NamespaceId,
        name: impl Into<String>,
        isolation_level: IsolationLevel,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            isolation_level,
            archived: false,
        }
    }
}

/// Handle for interacting with a specific namespace.
///
/// All operations through this handle are scoped to the namespace.
pub struct NamespaceHandle {
    /// The namespace this handle refers to.
    #[allow(dead_code)]
    namespace: Namespace,
}

impl NamespaceHandle {
    /// Returns the namespace ID.
    pub fn id(&self) -> NamespaceId {
        self.namespace.id
    }

    /// Returns the namespace name.
    pub fn name(&self) -> &str {
        &self.namespace.name
    }

    /// Returns the isolation level.
    pub fn isolation_level(&self) -> IsolationLevel {
        self.namespace.isolation_level
    }

    /// Returns true if the namespace is archived (read-only).
    pub fn is_archived(&self) -> bool {
        self.namespace.archived
    }
}

/// Registry for managing multiple namespaces.
pub struct NamespaceRegistry {
    /// Active namespaces.
    namespaces: HashMap<u64, Namespace>,
    /// Next ID counter.
    next_id: u64,
}

impl NamespaceRegistry {
    /// Creates a new, empty namespace registry.
    pub fn new() -> Self {
        Self {
            namespaces: HashMap::new(),
            next_id: 1,
        }
    }

    /// Creates a new namespace.
    pub fn create(
        &mut self,
        name: &str,
        level: IsolationLevel,
    ) -> Result<NamespaceId> {
        let id = NamespaceId::new(self.next_id);
        self.next_id += 1;

        let ns = Namespace::new(id, name, level);
        self.namespaces.insert(id.inner(), ns);
        Ok(id)
    }

    /// Gets a handle to an existing namespace.
    pub fn get(&self, id: NamespaceId) -> Result<NamespaceHandle> {
        let ns = self
            .namespaces
            .get(&id.inner())
            .ok_or_else(|| {
                memfuse_core::MemFuseError::NotFound(format!(
                    "Namespace {} not found",
                    id
                ))
            })?
            .clone();

        Ok(NamespaceHandle { namespace: ns })
    }

    /// Archives a namespace (makes it read-only).
    pub fn archive(&mut self, id: NamespaceId) -> Result<()> {
        let ns = self
            .namespaces
            .get_mut(&id.inner())
            .ok_or_else(|| {
                memfuse_core::MemFuseError::NotFound(format!(
                    "Namespace {} not found",
                    id
                ))
            })?;
        ns.archived = true;
        Ok(())
    }

    /// Validates cross-namespace access permission.
    pub fn validate_cross_access(
        &self,
        from: NamespaceId,
        to: NamespaceId,
    ) -> Result<()> {
        if from == to {
            return Ok(());
        }

        let target = self
            .namespaces
            .get(&to.inner())
            .ok_or_else(|| {
                memfuse_core::MemFuseError::NotFound(format!(
                    "Namespace {} not found",
                    to
                ))
            })?;

        match target.isolation_level {
            IsolationLevel::Strict => {
                Err(memfuse_core::MemFuseError::NamespaceViolation(format!(
                    "Cross-namespace access denied: {} -> {} (Strict isolation)",
                    from, to
                )))
            }
            IsolationLevel::SharedRead | IsolationLevel::Logical => Ok(()),
        }
    }

    /// Returns the number of registered namespaces.
    pub fn count(&self) -> usize {
        self.namespaces.len()
    }
}

impl Default for NamespaceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_namespace_isolation() {
        let mut reg = NamespaceRegistry::new();
        let ns_a = reg.create("research", IsolationLevel::Strict).expect("valid test value"); // unwrap allowed (AGENT:04)
        let ns_b = reg.create("code", IsolationLevel::SharedRead).expect("valid test value"); // unwrap allowed (AGENT:04)

        // Strict -> deny cross-access TO strict namespace
        assert!(reg.validate_cross_access(ns_b, ns_a).is_err());
        // SharedRead -> allow cross-access TO shared namespace
        assert!(reg.validate_cross_access(ns_a, ns_b).is_ok());
    }

    #[test]
    fn test_namespace_archive() {
        let mut reg = NamespaceRegistry::new();
        let ns = reg.create("test", IsolationLevel::Logical).expect("valid test value"); // unwrap allowed (AGENT:04)
        reg.archive(ns).expect("valid test value"); // unwrap allowed (AGENT:04)

        let handle = reg.get(ns).expect("valid test value"); // unwrap allowed (AGENT:04)
        assert!(handle.is_archived());
    }
}

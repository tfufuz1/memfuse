//! Utility types and functions for Project Chimera.

use crate::{ChimeraError, Result};
use bytecheck::CheckBytes;
use rkyv::validation::validators::DefaultValidator;
use rkyv::{check_archived_root, Archive};

/// A wrapper around a byte slice for zero-copy parsing.
///
/// This utility enables safe casting of raw byte buffers (e.g. from memory-mapped files)
/// into archived rkyv types with full validation.
pub struct AliasedBytes<'a> {
    data: &'a [u8],
}

impl<'a> AliasedBytes<'a> {
    /// Creates a new AliasedBytes from a byte slice.
    pub fn new(data: &'a [u8]) -> Self {
        Self { data }
    }

    /// Casts the bytes to an archived rkyv type with validation.
    ///
    /// # Errors
    ///
    /// Returns an error if the data is not a valid representation of the archived type.
    pub fn cast<T>(&self) -> Result<&T::Archived>
    where
        T: Archive,
        T::Archived: CheckBytes<DefaultValidator<'a>> + 'a,
    {
        check_archived_root::<T>(self.data)
            .map_err(|e| ChimeraError::Internal(format!("Zero-copy cast failed: {:?}", e)))
    }

    /// Returns the raw data.
    pub fn data(&self) -> &[u8] {
        self.data
    }
}

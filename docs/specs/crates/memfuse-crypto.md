# Agent Specification: memfuse-crypto

**Agent**: @JULES-06
**Domain**: Encryption at Rest & Key Derivation
**Status**: 🔴 Vulnerable (Cryptographic Nonce-Reuse)

## Mission Statement
Guarantee that WAL, LSM, and Snapshot state on disk cannot be compromised. Ensure non-repeatability of Nonces using HKDF key derivation.

## Critical Remediation Targets
1. **Nonce Reuse Shielding (`FIND-CRY-002`)**: Implement sub-key derivation utilizing HKDF. Never construct nonces globally or re-use them across WAL blocks.
2. **Salt Generation (`FIND-CRY-001`)**: Strengthen automatic salt derivation and persistence.

# Sentinel's Journal - Critical Security Learnings

## 2026-06-15 - Information Leak in Cryptographic Error Messages
**Vulnerability:** The WAL (Write-Ahead Log) implementation was leaking internal cryptographic states (HMAC values and CRC32 checksums) in error messages when corruption was detected.
**Learning:** Error messages that include "expected" vs "actual" values for hashes or checksums can assist an attacker in bit-flipping attacks or in analyzing the internal state of the system without having direct access to the memory or the keys. Even if the keys themselves aren't leaked, leaking the resulting HMACs provides a side-channel for verifying guesses about the data.
**Prevention:** Always use generic error messages for integrity failures in production code. If detailed information is needed for debugging, it should be restricted to internal tracing/logging at a sensitive level, never exposed through public error enums or displayed to the user. Identification of the *location* of the error (e.g., sequence number or file offset) is usually sufficient for recovery without leaking the cryptographic values themselves.

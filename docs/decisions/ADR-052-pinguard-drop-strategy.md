# ADR-052: PinGuard Drop Strategy

**Status:** Accepted
**Date:** 2026-09-03

## Decision

Replace fire-and-forget `thread::spawn` in `PinGuard::drop` with a synchronous orphan registration into a persistent store. Orphans are recovered on next startup.

## Rationale

`thread::spawn` is not reliable at drop time (runtime shutdown, resource exhaustion). A durable orphan record survives crashes and is processed on restart.

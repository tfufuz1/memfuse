//! Inter-Process Communication (IPC) protocol definitions for `MemFuse`.
//!
//! # `FlatBuffers` Code Generation
//! The Rust code for FlatBuffers schemas is **auto-generated** in [`memfuse_core_ipc_gen`]
//! from the FlatBuffers schema located at `schemas/memfuse.fbs`.

pub mod jsonrpc;

pub use jsonrpc::*;
pub use memfuse_core_ipc_gen::*;

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn prop_ipc_parser_no_panic_on_garbage(bytes in proptest::collection::vec(any::<u8>(), 0..2048)) {
            // Dies beweist, dass der Parser fehlerhafte, zufällige, abgeschnittene
            // oder überlange Bytes sauber über Result zurückgibt und niemals panikt.
            let _ = root_as_search_response(&bytes);
        }
    }

    #[test]
    fn test_ipc_parser_empty() {
        let res = root_as_search_response(&[]);
        assert!(res.is_err());
    }

    #[test]
    fn test_ipc_parser_truncated() {
        let res = root_as_search_response(&[0x00, 0x01, 0x02]);
        assert!(res.is_err());
    }
}

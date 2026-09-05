//! Build identifier for split-debug-symbols (zbc 1.2 / zpkg 0.3+).
//!
//! A build_id is a 16-byte content tag the compiler writes into BOTH the main
//! binary's `BLID` section and its sidecar (`.zsym`), so the loader can tell
//! whether a given `.zsym` belongs to a given `.zpkg`. The writer computes it
//! over the whole main file with the BLID payload (the trailing 16 bytes)
//! zeroed; see `Z42.Project.ZpkgWriterZ.WritePackedWithSidecar`.
//!
//! **The runtime never recomputes it** — pairing is a plain equality check
//! between the two stored values (`read_build_id` + `!=` in
//! `loader::artifact`). It is therefore not a security boundary, and the
//! writer deliberately uses a fast non-cryptographic hash (MurmurHash3
//! x86_128) rather than BLAKE3: z42c runs interpreted, where BLAKE3 cost ~15x
//! more. Because this side only compares, the algorithm lives in exactly one
//! place — the writer — and there is intentionally no `compute()` here to
//! drift out of sync with it.
//!
//! Not to be confused with an indexed zpkg's scattered-`.zbc` `zbc_hash`,
//! which IS recomputed here (plain BLAKE3-128, `loader::artifact`) and so is a
//! real cross-language contract.

pub const SIZE: usize = 16;

/// Formats the first 4 bytes of a build_id as 8 lowercase hex chars,
/// matching the trace fallback `[build:abcd1234]` suffix.
pub fn short_hex(build_id: &[u8]) -> String {
    assert!(build_id.len() >= 4, "build_id must be at least 4 bytes");
    format!(
        "{:02x}{:02x}{:02x}{:02x}",
        build_id[0], build_id[1], build_id[2], build_id[3],
    )
}

#[cfg(test)]
#[path = "build_id_tests.rs"]
mod build_id_tests;

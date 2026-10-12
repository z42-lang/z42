//! Compile-time test discovery — TestIndex section types and reader.
//!
//! z42 编译器在 zbc 文件中加 TIDX section（spec R1
//! `add-test-metadata-section`）。Section payload 含一个 `TestEntry` 数组，每条
//! 描述一个被 `[Test]` / `[Benchmark]` / `[Setup]` / `[Teardown]` 标注的
//! 函数。z42-test-runner（R3）从这里发现测试，**不再扫**整个 method table。
//!
//! 本模块只做**数据类型 + decoder**；编译器侧的 emit 与 attribute 识别在
//! C# 侧（`Z42.IR.Metadata.TestEntry`）。
//!
//! ### TIDX section payload (little-endian, fixed-width)
//!
//! Uses fixed-width LE integers (u32 / u16 / u8) for consistency with other
//! zbc sections (NSPC / FUNC / DBUG / etc.).
//!
//! ```text
//! u32   magic = 0x58_44_49_54 ("TIDX" on disk: 54 49 44 58)
//! u8    version = 2
//! u32   entry_count
//! TestEntry entries[entry_count]
//!
//! TestEntry (27 + 4*test_case_count bytes):
//!   u32   method_id               (index into module.functions[])
//!   u8    kind                    (TestEntryKind discriminant)
//!   u16   flags                   (TestFlags bitset)
//!   u32   skip_reason_str_idx     (0 = none; otherwise 1-based pool idx)
//!   u32   skip_platform_str_idx   (0 = always-skipped; otherwise platform name)
//!   u32   skip_feature_str_idx    (0 = no feature gate; otherwise feature name)
//!   u32   expected_throw_type_idx (0 = none; reserved for R4 — currently 0)
//!   u32   test_case_count
//!   TestCase test_cases[test_case_count]
//!
//! TestCase (4 bytes):
//!   u32   arg_repr_str_idx        (1-based string pool idx)
//! ```
//!
//! ### Version history
//!
//! - **v=1** (R1.A+B, 2026-04-29): initial format with `skip_reason_str_idx`
//!   only. Bumped to v=2 in R1.C before any v=1 file existed in the wild
//!   (parser support didn't ship with R1.A+B, so no .zbc was ever written
//!   with TIDX entries; all v=1 .zbc had empty TIDX or no TIDX section).
//! - **v=2** (R1.C): added `skip_platform_str_idx` + `skip_feature_str_idx`
//!   for conditional skip ([Skip(platform: "ios", feature: "jit", reason:
//!   "...")] semantics).
//!
//! ### Magic byte order note
//!
//! On disk the bytes are `54 49 44 58` ("T I D X"). When read as a little-endian
//! u32 those bytes become `0x58444954`. We read 4 bytes raw and compare against
//! the byte slice; reading via `read_u32_le` expects the LE-decoded value
//! `0x58444954`. The constant `TEST_INDEX_MAGIC` matches the LE-decoded form to
//! match how the reader currently does its comparison.

use anyhow::{anyhow, bail, Result};
use bitflags::bitflags;

/// Magic value at the start of the TIDX section, after little-endian u32 read.
/// On-disk bytes are `T I D X` (54 49 44 58).
pub const TEST_INDEX_MAGIC: u32 = 0x58_44_49_54;

/// Current TIDX section format version. Bumped on incompatible payload changes.
/// v=3 (add-test-timeout-attribute, 2026-05-30): TestEntry gains a trailing
/// `timeout_ms: i32` after its TestCase array. 0 = no override (runner uses
/// its built-in default); positive = per-test wallclock cap in ms.
pub const TEST_INDEX_VERSION: u8 = 3;

/// Test-method classification, mirrored from C# `TestEntryKind`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TestEntryKind {
    /// `[Test]` — regular test method.
    Test      = 1,
    /// `[Benchmark]` — measurement method (runner uses different scheduler).
    Benchmark = 2,
    /// `[Setup]` — runs before each [Test] in the same module.
    Setup     = 3,
    /// `[Teardown]` — runs after each [Test] in the same module.
    Teardown  = 4,
    /// `[Doctest]` — extracted from `///` doc comment; reserved (v0.2).
    Doctest   = 5,
}

impl TestEntryKind {
    /// Decode a u8 discriminant. Returns Err for unknown values to surface
    /// forward-compat issues at load time.
    pub fn from_u8(b: u8) -> Result<Self> {
        match b {
            1 => Ok(Self::Test),
            2 => Ok(Self::Benchmark),
            3 => Ok(Self::Setup),
            4 => Ok(Self::Teardown),
            5 => Ok(Self::Doctest),
            other => bail!("unknown TestEntryKind discriminant: {other}"),
        }
    }
}

bitflags! {
    /// Boolean flags on a [`TestEntry`]. Reserved bits (4-15) must be zero.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub struct TestFlags: u16 {
        /// Has [Skip(reason)]. `skip_reason_str_idx` references the reason.
        const SKIPPED      = 1 << 0;
        /// Has [Ignore]. Runner does not list this entry.
        const IGNORED      = 1 << 1;
        /// Has [ShouldThrow<E>]. `expected_throw_type_idx` references the
        /// expected exception type (reserved for R4).
        const SHOULD_THROW = 1 << 2;
        /// Reserved (v0.2 doctest pipeline).
        const DOCTEST      = 1 << 3;
    }
}

/// One parameterized variant of a `[TestCase(args)]`-decorated method. R1 stores
/// the args as a single string literal (their textual representation); R4 will
/// upgrade to a typed encoding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestCase {
    /// 1-based index into module string pool; arg representation as a single
    /// string literal (e.g. `"0, 0"`).
    pub arg_repr_str_idx: u32,
}

/// One row in the TIDX section.
///
/// **String resolution lifecycle**: `*_str_idx` fields point into the
/// **raw STRS pool** (pre-rebuild). The loader (see
/// [`crate::metadata::loader::load_zbc`]) resolves them to the `*_resolved`
/// `Option<String>` fields immediately after `read_zbc`, **before** the global
/// pool gets rebuilt by `rebuild_string_pool` (which only retains strings
/// referenced by `ConstStr` instructions, dropping TIDX-only strings).
///
/// **Runner code should read the resolved strings** (`skip_reason`, etc.) and
/// ignore the `_str_idx` fields. The indices stay around for round-tripping
/// against the cross-language contract test.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEntry {
    /// Index into `module.functions[]` in the same .zbc.
    pub method_id: u32,
    /// Test classification.
    pub kind: TestEntryKind,
    /// Bit flags ([Skip] / [Ignore] / [ShouldThrow] / reserved).
    pub flags: TestFlags,
    /// 0 = no [Skip] reason; otherwise 1-based index into the **raw** STRS pool.
    pub skip_reason_str_idx: u32,
    /// 0 = unconditionally skipped (or no [Skip]); otherwise 1-based idx of
    /// platform name (e.g. "ios", "wasm") — runner only skips when running
    /// on this platform. Lets you tag tests that don't apply to certain OSes.
    pub skip_platform_str_idx: u32,
    /// 0 = no feature gate; otherwise 1-based idx of feature name (e.g.
    /// "jit", "multithreading") — runner skips when this feature is
    /// unavailable in the running build configuration.
    pub skip_feature_str_idx: u32,
    /// 0 = no [ShouldThrow<E>]; otherwise 1-based index (R4 will fill).
    pub expected_throw_type_idx: u32,
    /// Empty when not parameterized; one element per `[TestCase(...)]`.
    pub test_cases: Vec<TestCase>,
    /// add-test-timeout-attribute (2026-05-30): per-test wallclock budget
    /// in milliseconds. 0 = no override (runner uses its built-in default —
    /// currently 300 s); positive = explicit cap from
    /// `[Timeout(milliseconds: N)]`. Always > 0 when non-zero (compile-time
    /// E0916 enforces 0 &lt; N ≤ i32::MaxValue).
    pub timeout_ms: u32,

    // ── Resolved strings (populated by loader::resolve_test_index_strings) ──
    /// Resolved [Skip(reason: ...)] string. None when `skip_reason_str_idx == 0`
    /// or before resolution.
    pub skip_reason: Option<String>,
    /// Resolved [Skip(platform: ...)] string.
    pub skip_platform: Option<String>,
    /// Resolved [Skip(feature: ...)] string.
    pub skip_feature: Option<String>,
    /// Resolved [ShouldThrow<E>] type name (R4 will populate).
    pub expected_throw_type: Option<String>,
}

/// Resolve all `*_str_idx` fields in `entries` against `raw_pool` (the STRS
/// section as decoded from disk, **before** `rebuild_string_pool` discards
/// strings only referenced by TIDX). Populates the corresponding `Option<String>`
/// fields in-place. Indices remain unchanged for round-trip diagnostics.
///
/// Called by [`crate::metadata::loader`] after `read_zbc` and before
/// `rebuild_string_pool` runs (which would lose TIDX-only strings).
pub fn resolve_test_index_strings(entries: &mut [TestEntry], raw_pool: &[String]) {
    let lookup = |idx_1based: u32| -> Option<String> {
        if idx_1based == 0 { return None; }
        raw_pool.get((idx_1based - 1) as usize).cloned()
    };
    for e in entries.iter_mut() {
        e.skip_reason         = lookup(e.skip_reason_str_idx);
        e.skip_platform       = lookup(e.skip_platform_str_idx);
        e.skip_feature        = lookup(e.skip_feature_str_idx);
        e.expected_throw_type = lookup(e.expected_throw_type_idx);
    }
}

// ── Decoder ────────────────────────────────────────────────────────────────

/// Decode a TIDX section payload (without the file-level header / directory
/// wrapper — caller already extracted the section bytes).
///
/// Returns an empty Vec if the section is empty (no entries). Returns Err on
/// malformed magic, unsupported version, or truncated input.
pub fn read_test_index(payload: &[u8]) -> Result<Vec<TestEntry>> {
    let mut cursor = TidxCursor::new(payload);

    let magic = cursor.read_u32_le()?;
    if magic != TEST_INDEX_MAGIC {
        bail!(
            "invalid TIDX magic: expected 0x{:08x} (\"TIDX\"), got 0x{:08x}",
            TEST_INDEX_MAGIC, magic
        );
    }

    let version = cursor.read_u8()?;
    if version != TEST_INDEX_VERSION {
        bail!(
            "unsupported TIDX version: expected {}, got {}",
            TEST_INDEX_VERSION, version
        );
    }

    let entry_count = cursor.read_u32_le()?;
    let mut entries = Vec::with_capacity(entry_count as usize);
    for _ in 0..entry_count {
        let method_id              = cursor.read_u32_le()?;
        let kind                   = TestEntryKind::from_u8(cursor.read_u8()?)?;
        let flags_raw              = cursor.read_u16_le()?;
        let flags = TestFlags::from_bits(flags_raw)
            .ok_or_else(|| anyhow!("TIDX entry has reserved flag bits set: 0x{:04x}", flags_raw))?;
        let skip_reason_str_idx    = cursor.read_u32_le()?;
        let skip_platform_str_idx  = cursor.read_u32_le()?;
        let skip_feature_str_idx   = cursor.read_u32_le()?;
        let expected_throw_type_idx = cursor.read_u32_le()?;
        let test_case_count        = cursor.read_u32_le()?;
        let mut test_cases = Vec::with_capacity(test_case_count as usize);
        for _ in 0..test_case_count {
            let arg_repr_str_idx = cursor.read_u32_le()?;
            test_cases.push(TestCase { arg_repr_str_idx });
        }
        // add-test-timeout-attribute (2026-05-30): TIDX v=3 trailing
        // `timeout_ms i32`. Negative not expected (compile-time E0917
        // enforces > 0), but defensively saturate to 0 = "no override"
        // so an old-format / corrupt file degrades to runner default
        // instead of integer-underflow on the u32 cast.
        let timeout_ms_i32 = cursor.read_i32_le()?;
        let timeout_ms = if timeout_ms_i32 > 0 { timeout_ms_i32 as u32 } else { 0 };
        entries.push(TestEntry {
            method_id,
            kind,
            flags,
            skip_reason_str_idx,
            skip_platform_str_idx,
            skip_feature_str_idx,
            expected_throw_type_idx,
            test_cases,
            timeout_ms,
            // Resolved fields populated by loader::resolve_test_index_strings
            skip_reason: None,
            skip_platform: None,
            skip_feature: None,
            expected_throw_type: None,
        });
    }

    Ok(entries)
}

// ── Cursor helpers (private) ────────────────────────────────────────────────

struct TidxCursor<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl<'a> TidxCursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, pos: 0 }
    }

    fn read_u8(&mut self) -> Result<u8> {
        if self.pos >= self.bytes.len() {
            bail!("TIDX truncated: expected 1 byte at offset {}", self.pos);
        }
        let b = self.bytes[self.pos];
        self.pos += 1;
        Ok(b)
    }

    fn read_u16_le(&mut self) -> Result<u16> {
        if self.pos + 2 > self.bytes.len() {
            bail!("TIDX truncated: expected 2 bytes at offset {}", self.pos);
        }
        let v = u16::from_le_bytes([self.bytes[self.pos], self.bytes[self.pos + 1]]);
        self.pos += 2;
        Ok(v)
    }

    fn read_u32_le(&mut self) -> Result<u32> {
        if self.pos + 4 > self.bytes.len() {
            bail!("TIDX truncated: expected 4 bytes at offset {}", self.pos);
        }
        let v = u32::from_le_bytes([
            self.bytes[self.pos],
            self.bytes[self.pos + 1],
            self.bytes[self.pos + 2],
            self.bytes[self.pos + 3],
        ]);
        self.pos += 4;
        Ok(v)
    }

    /// add-test-timeout-attribute (2026-05-30): signed 32-bit reader for
    /// `timeout_ms`. C# writer emits `BinaryWriter.Write(int)` which is
    /// little-endian two's-complement; matching read here.
    fn read_i32_le(&mut self) -> Result<i32> {
        Ok(self.read_u32_le()? as i32)
    }
}

/// A runtime-loaded test entry, returned by `__test_module_load` (the seed SDK
/// `__load_module` builtin) to a z42 test runner. Flattens a [`TestEntry`] + its
/// resolved method FQN into a self-contained, z42-facing record so the runner can
/// discover, classify, and `Invoke` each test reflectively without reaching back
/// into VM metadata. (retire-test-runner)
#[derive(Debug, Clone)]
pub struct LoadedTestEntry {
    /// Fully-qualified method name (`<FQClass>.<Method>`), for `Type.GetType` +
    /// `MethodInfo.Invoke`. Empty if `method_id` was out of range.
    pub qualified: String,
    /// `TestEntryKind` discriminant (1=Test, 2=Benchmark, 3=Setup, 4=Teardown).
    pub kind: u8,
    /// `TestFlags` bits (SKIPPED / IGNORED / SHOULD_THROW).
    pub flags: u16,
    pub skip_reason: Option<String>,
    pub skip_platform: Option<String>,
    pub skip_feature: Option<String>,
    /// `[ShouldThrow<E>]` expected exception type FQN, if any.
    pub expected_throw: Option<String>,
}

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "test_index_tests.rs"]
mod test_index_tests;

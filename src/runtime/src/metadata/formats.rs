/// File-format constants shared by the `.zbc` / `.zpkg` readers, plus the
/// dependency record carried in a loaded artifact.

/// Magic bytes for `.zbc` binary format: `"ZBC\0"`
pub const ZBC_MAGIC: [u8; 4] = [0x5A, 0x42, 0x43, 0x00];

/// Magic bytes for `.zpkg` binary format: `"ZPK\0"`
pub const ZPKG_MAGIC: [u8; 4] = [0x5A, 0x50, 0x4B, 0x00];

/// A dependency of a package: the file that provided some of the namespaces it
/// uses. Records the actual file, not a declarative constraint.
#[derive(Debug, Clone)]
pub struct ZpkgDep {
    /// Filename of the dependency (e.g. `"z42-io.zpkg"` or `"utils.zbc"`).
    pub file: String,
    /// Namespaces provided by this dependency that are used by this package.
    pub namespaces: Vec<String>,
    /// precise-pkg-refs (zpkg 0.52): full names of the types / free functions this
    /// package references that are **defined by** this dependency (symbol keys —
    /// generic args and `$` suffixes stripped, see `lazy_loader::symbol_key`). The
    /// lazy loader routes a missing name straight to the package that defines it,
    /// like a .NET TypeRef's resolution scope, instead of guessing by namespace.
    pub symbols: Vec<String>,
}

//! Read-only source for a program's dependencies and startup configuration.

use std::path::Path;

use crate::zpkg::format::{ZBC_MAGIC, ZPKG_MAGIC};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum FileKind {
    Zpkg = 1,
    Zbc = 2,
    RuntimeConfig = 3,
    BytecodeDependencies = 4,
}

impl FileKind {
    pub(super) fn for_dependency(name: &str) -> anyhow::Result<Self> {
        anyhow::ensure!(
            !name.contains(['\\', ':', '\0', '\n', '\r'])
                && !name.split('/').any(|part| matches!(part, "" | "." | "..")),
            "invalid dependency file name `{name}`",
        );
        match Path::new(name).extension().and_then(|suffix| suffix.to_str()) {
            Some("zpkg") => Ok(Self::Zpkg),
            Some("zbc") => Ok(Self::Zbc),
            _ => anyhow::bail!("unsupported dependency file name `{name}`"),
        }
    }

    pub(super) fn validate_code(self, bytes: &[u8]) -> bool {
        match self {
            Self::Zpkg => bytes.starts_with(&ZPKG_MAGIC),
            Self::Zbc => bytes.starts_with(&ZBC_MAGIC),
            Self::RuntimeConfig | Self::BytecodeDependencies => false,
        }
    }
}

/// Supply bytes for the given exact logical name. A miss allows filesystem lookup.
pub trait FileResolver: Send + Sync {
    fn resolve(&self, name: &str, kind: FileKind) -> anyhow::Result<Option<Vec<u8>>>;
}

/// A standalone .zbc has a separate list of exact dependency names.
pub(super) fn bytecode_dependencies(
    resolver: &dyn FileResolver,
    name: &str,
    fs: &dyn crate::pal::fs::Vfs,
    fallback_program: &str,
) -> anyhow::Result<Vec<String>> {
    let manifest = format!("{name}.deps");
    let bytes = match resolver.resolve(&manifest, FileKind::BytecodeDependencies)? {
        Some(bytes) => bytes,
        None => match fs.read(&format!("{fallback_program}.deps")) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        },
    };
    let text = std::str::from_utf8(&bytes)
        .map_err(|error| anyhow::anyhow!("invalid UTF-8 in `{manifest}`: {error}"))?;
    text.lines()
        .map(|file| {
            FileKind::for_dependency(file)?;
            Ok(file.to_owned())
        })
        .collect()
}

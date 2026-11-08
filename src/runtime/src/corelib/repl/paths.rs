//! Candidate path policy supplied by the facade, not captured at VM construction.
use crate::vm::context::VmContext;
use std::path::PathBuf;

pub(crate) type Source = fn() -> Vec<PathBuf>;
pub(crate) struct Paths(pub(crate) Source);

pub(crate) fn source(ctx: &VmContext) -> anyhow::Result<Source> {
    ctx.ext::<Paths>()
        .map(|paths| paths.0)
        .ok_or_else(|| anyhow::anyhow!("REPL native path service was not registered"))
}

#[cfg(test)]
#[path = "paths_tests.rs"]
mod tests;

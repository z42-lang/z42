//! Result shared by all VM execution engines.
use crate::object::Value;

/// Outcome of executing a function.
/// User exceptions are value-based (no heap allocation), not anyhow errors.
///
/// Public so embedders (test-runner, REPL) can introspect thrown exception
/// values — necessary for [ShouldThrow<E>] type matching and TestFailure /
/// SkipSignal classification (rewrite-z42-test-runner-compile-time S3).
#[derive(Debug)]
pub enum ExecOutcome {
    /// Normal return (with optional return value).
    Returned(Option<Value>),
    /// User exception thrown and not caught within this function.
    Thrown(Value),
}


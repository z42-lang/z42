//! Typed builtin errors and the one place they become a z42 exception or a fatal VM
//! error — shared by the interpreter (`exec_call`) and the JIT (`jit_builtin`, the
//! `[Native]` forward short-circuit), so both engines throw the same thing.

use crate::exception::FrameSnapshot;
use crate::vm::runtime::Module;
use crate::metadata::Value;
use crate::vm_context::VmContext;

/// What a builtin can fail with. Every kind but `Internal` / `Thrown` is one stdlib
/// exception class (.NET names and hierarchy — [`BuiltinError::classes`]); the message
/// is the exception's `Message`.
#[derive(Debug)]
pub enum BuiltinError {
    /// `Std.FormatException` — text not in the expected format (`Int32.Parse("x")`).
    Format(String),
    /// `Std.ArgumentException`.
    Argument(String),
    /// `Std.ArgumentNullException : ArgumentException` — a null argument where a value is required.
    ArgumentNull(String),
    /// `Std.ArgumentOutOfRangeException : ArgumentException`.
    ArgumentOutOfRange(String),
    /// `Std.IndexOutOfRangeException`.
    IndexOutOfRange(String),
    /// `Std.OverflowException : ArithmeticException` (`Int32.Parse("99999999999")`).
    Overflow(String),
    /// `Std.InvalidOperationException`.
    InvalidOperation(String),
    /// `Std.NotSupportedException`.
    NotSupported(String),
    /// `Std.InvalidCastException` — an argument of another kind than the parameter takes.
    InvalidCast(String),
    /// `Std.NullReferenceException` — a null receiver.
    NullReference(String),
    /// `Std.KeyNotFoundException`.
    KeyNotFound(String),
    /// `Std.IO.IOException`.
    Io(String),
    /// `Std.IO.FileNotFoundException : IOException`.
    FileNotFound(String),
    /// `Std.IO.DirectoryNotFoundException : IOException`.
    DirectoryNotFound(String),
    /// `Std.IO.PathTooLongException : IOException`.
    PathTooLong(String),
    /// `Std.IO.EndOfStreamException : IOException` (lives in `z42.io`).
    EndOfStream(String),
    /// `Std.UnauthorizedAccessException`.
    UnauthorizedAccess(String),
    /// `Std.TimeoutException`.
    Timeout(String),
    /// Any other stdlib / package exception class by fully qualified name
    /// (`Std.Threading.ThreadException`). Falls back to `Std.Exception` when the class
    /// is not loaded.
    Exception { class: &'static str, message: String },
    /// A class with fallbacks (most specific first, as [`BuiltinError::classes`]) and
    /// public fields besides `Message` — a package exception that carries data
    /// (`Std.IO.ProcessTimeoutException.Program` / `TimeoutMs`). A field the class that
    /// gets built does not declare (a fallback base class) is skipped.
    Detailed(Box<Detailed>),
    /// An exception value already built (a z42 callback threw it) — rethrown as is.
    Thrown(Value),
    /// A VM bug / broken invariant (a `[Native]` declaration that does not match the
    /// Rust signature, a corrupted handle, "cannot happen"). **Fatal**: not catchable —
    /// the VM stops with a report, like a stack overflow.
    Internal(String),
    /// An error from a builtin of the old table (`anyhow`): the old mapping — a typed
    /// `objops::Throw` keeps its class, anything else becomes `Std.Exception`.
    Legacy(anyhow::Error),
}

/// Payload of [`BuiltinError::Detailed`].
#[derive(Debug)]
pub struct Detailed {
    pub classes: &'static [&'static str],
    pub message: String,
    pub fields: Vec<(&'static str, FieldInit)>,
}

/// A field value of a [`Detailed`] exception (plain data; built into a `Value` when the
/// exception is).
#[derive(Debug, Clone)]
pub enum FieldInit {
    Str(String),
    I64(i64),
}

impl BuiltinError {
    /// A [`BuiltinError::Detailed`] exception.
    pub fn detailed(
        classes: &'static [&'static str], message: impl Into<String>, fields: Vec<(&'static str, FieldInit)>,
    ) -> Self {
        BuiltinError::Detailed(Box::new(Detailed { classes, message: message.into(), fields }))
    }

    /// `Std.ObjectDisposedException : InvalidOperationException` (falls back to
    /// `InvalidOperationException`), `ObjectName` = `object_name` (null when `None`).
    pub fn object_disposed(object_name: Option<&str>, message: impl Into<String>) -> Self {
        const CLASSES: &[&str] = &["Std.ObjectDisposedException", "Std.InvalidOperationException"];
        let fields = object_name.map(|n| vec![("ObjectName", FieldInit::Str(n.into()))]).unwrap_or_default();
        BuiltinError::detailed(CLASSES, message, fields)
    }

    /// The argument count does not match the builtin — the `[Native]` declaration and
    /// the Rust signature disagree (a VM / stdlib bug, hence internal).
    #[cold]
    pub fn arity(builtin: &str, want: usize, got: usize) -> Self {
        BuiltinError::Internal(format!(
            "`{builtin}` takes {want} argument(s), the call passes {got} — the stdlib `[Native]` \
             declaration and the Rust builtin disagree"))
    }

    /// The exception classes this error throws, most specific first. The first one that
    /// is loaded is used; `Std.Exception` is the last resort.
    pub fn classes(&self) -> &'static [&'static str] {
        use BuiltinError::*;
        match self {
            Format(_) => &["Std.FormatException"],
            Argument(_) => &["Std.ArgumentException"],
            ArgumentNull(_) => &["Std.ArgumentNullException", "Std.ArgumentException"],
            ArgumentOutOfRange(_) => &["Std.ArgumentOutOfRangeException", "Std.ArgumentException"],
            IndexOutOfRange(_) => &["Std.IndexOutOfRangeException"],
            Overflow(_) => &["Std.OverflowException", "Std.ArithmeticException"],
            InvalidOperation(_) => &["Std.InvalidOperationException"],
            NotSupported(_) => &["Std.NotSupportedException"],
            InvalidCast(_) => &["Std.InvalidCastException"],
            NullReference(_) => &["Std.NullReferenceException"],
            KeyNotFound(_) => &["Std.KeyNotFoundException"],
            Io(_) => &["Std.IO.IOException"],
            FileNotFound(_) => &["Std.IO.FileNotFoundException", "Std.IO.IOException"],
            DirectoryNotFound(_) => &["Std.IO.DirectoryNotFoundException", "Std.IO.IOException"],
            PathTooLong(_) => &["Std.IO.PathTooLongException", "Std.IO.IOException"],
            EndOfStream(_) => &["Std.IO.EndOfStreamException", "Std.IO.IOException"],
            UnauthorizedAccess(_) => &["Std.UnauthorizedAccessException"],
            Timeout(_) => &["Std.TimeoutException"],
            Detailed(d) => d.classes,
            Exception { .. } | Thrown(_) | Internal(_) | Legacy(_) => &[],
        }
    }

    /// The exception message (empty for `Thrown`).
    pub fn message(&self) -> std::borrow::Cow<'_, str> {
        use BuiltinError::*;
        match self {
            Format(m) | Argument(m) | ArgumentNull(m) | ArgumentOutOfRange(m) | IndexOutOfRange(m)
            | Overflow(m) | InvalidOperation(m) | NotSupported(m) | InvalidCast(m) | NullReference(m)
            | KeyNotFound(m) | Io(m) | FileNotFound(m) | DirectoryNotFound(m) | PathTooLong(m)
            | EndOfStream(m) | UnauthorizedAccess(m) | Timeout(m) | Internal(m) => m.as_str().into(),
            Exception { message, .. } => message.as_str().into(),
            Detailed(d) => d.message.as_str().into(),
            Thrown(_) => "".into(),
            Legacy(e) => e.to_string().into(),
        }
    }

    /// Into the `anyhow` channel of callers that hold the old signature (`exec_builtin*`).
    /// `Thrown` travels through `pending_thrown`, like `corelib::raise_op`; the typed kinds
    /// stay downcastable ([`raise`] recovers them).
    pub fn into_anyhow(self, ctx: &VmContext) -> anyhow::Error {
        match self {
            BuiltinError::Thrown(v) => {
                ctx.set_pending_thrown(v);
                anyhow::anyhow!("exception")
            }
            BuiltinError::Legacy(e) => e,
            other => anyhow::Error::new(other),
        }
    }
}

impl std::fmt::Display for BuiltinError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuiltinError::Internal(m) => write!(f, "internal error: {m}"),
            BuiltinError::Thrown(_) => f.write_str("exception"),
            BuiltinError::Legacy(e) => write!(f, "{e}"),
            BuiltinError::Exception { class, message } => write!(f, "{class}: {message}"),
            BuiltinError::Detailed(d) => write!(f, "{}: {}", d.classes.first().copied().unwrap_or("Std.Exception"), d.message),
            other => write!(f, "{}: {}", other.classes()[0], other.message()),
        }
    }
}

impl std::error::Error for BuiltinError {}

impl From<std::io::Error> for BuiltinError {
    /// .NET's mapping of OS errors: missing path → `FileNotFoundException` (a builtin that
    /// knows it looked for a directory says `DirectoryNotFound` itself; path-based file
    /// system builtins map with the path instead — `corelib::fs::fs_error`), permission →
    /// `UnauthorizedAccessException`, timeout → `TimeoutException`, unsupported on this
    /// platform → `NotSupportedException`, the rest → `IOException`.
    fn from(e: std::io::Error) -> Self {
        use std::io::ErrorKind::*;
        let msg = e.to_string();
        match e.kind() {
            NotFound => BuiltinError::FileNotFound(msg),
            PermissionDenied => BuiltinError::UnauthorizedAccess(msg),
            TimedOut | WouldBlock => BuiltinError::Timeout(msg),
            InvalidInput => BuiltinError::Argument(msg),
            Unsupported => BuiltinError::NotSupported(msg),
            UnexpectedEof => BuiltinError::EndOfStream(msg),
            _ => BuiltinError::Io(msg),
        }
    }
}

impl From<crate::objops::OpError> for BuiltinError {
    fn from(e: crate::objops::OpError) -> Self {
        match e {
            crate::objops::OpError::Throw(t) => BuiltinError::Exception { class: t.class, message: t.msg },
            crate::objops::OpError::Thrown(v) => BuiltinError::Thrown(v),
            crate::objops::OpError::Internal(e) => BuiltinError::Internal(e.to_string()),
        }
    }
}

/// What the engine does with a failed builtin call.
pub(crate) enum Raised {
    /// Throw this z42 exception (its `StackTrace` is already filled).
    Throw(Value),
    /// A fatal VM error is unwinding: no z42 handler may run (`stack_guard`).
    Fatal(anyhow::Error),
    /// The exception class is not loaded (bare-module unit tests): report the raw error.
    Raw(anyhow::Error),
}

/// Turn a failed builtin call into what the engine propagates. `builtin` names the
/// builtin (for an internal error's report); `extra_rows` are the stack-trace rows of
/// the `[Native]` wrapper(s) a short-circuited call skipped, innermost last.
///
/// Order: an exception a callback stashed (`pending_thrown`) wins; while a fatal error
/// unwinds nothing becomes an exception; `Internal` starts a fatal error; every other
/// kind is built as its class and gets its `StackTrace` here — at the builtin, so the
/// trace is the same in both engines and, for a short-circuited call, still shows the
/// wrapper's row.
#[cold]
#[inline(never)]
pub(crate) fn raise(
    ctx: &VmContext, module: &Module, builtin: &str, e: BuiltinError, extra_rows: &[FrameSnapshot],
) -> Raised {
    if let Some(thrown) = ctx.take_pending_thrown() {
        crate::exception::populate_stack_trace_with(&thrown, ctx, module, extra_rows);
        return Raised::Throw(thrown);
    }
    if crate::stack_guard::is_fatal(ctx) {
        return Raised::Fatal(anyhow::anyhow!("{e}"));
    }
    match build(ctx, module, builtin, e) {
        Ok(exc) => {
            crate::exception::populate_stack_trace_with(&exc, ctx, module, extra_rows);
            Raised::Throw(exc)
        }
        Err(r) => r,
    }
}

fn build(ctx: &VmContext, module: &Module, builtin: &str, e: BuiltinError) -> Result<Value, Raised> {
    let classes = e.classes();
    match e {
        BuiltinError::Internal(msg) => Err(Raised::Fatal(crate::stack_guard::fatal_error(
            ctx, &format!("internal error in builtin `{builtin}`: {msg}")))),
        BuiltinError::Thrown(v) => Ok(v),
        BuiltinError::Legacy(err) => match err.downcast::<BuiltinError>() {
            Ok(typed) => build(ctx, module, builtin, typed),
            Err(err) => match err.downcast::<crate::objops::Throw>() {
                Ok(t) => make(ctx, module, &[t.class], t.msg),
                Err(err) => crate::exception::make_stdlib_exception(ctx, module, "Std.Exception", err.to_string())
                    .map_err(|_| Raised::Raw(err)),
            },
        },
        BuiltinError::Exception { class, message } => make(ctx, module, &[class], message),
        BuiltinError::Detailed(d) => {
            let fields: Vec<(&str, Value)> = d.fields.iter().map(|(n, v)| (*n, match v {
                FieldInit::Str(s) => Value::Str(s.as_str().into()),
                FieldInit::I64(n) => Value::I64(*n),
            })).collect();
            make_with(ctx, module, d.classes, d.message, &fields)
        }
        other => {
            let msg = other.message().into_owned();
            make(ctx, module, classes, msg)
        }
    }
}

/// The first loaded class of `classes`, else `Std.Exception`, with `msg`.
fn make(ctx: &VmContext, module: &Module, classes: &[&str], msg: String) -> Result<Value, Raised> {
    make_with(ctx, module, classes, msg, &[])
}

/// [`make`] plus extra fields (those the built class declares).
fn make_with(
    ctx: &VmContext, module: &Module, classes: &[&str], msg: String, fields: &[(&str, Value)],
) -> Result<Value, Raised> {
    for class in classes.iter().copied().chain(std::iter::once("Std.Exception")) {
        if let Ok(v) = crate::exception::make_stdlib_exception_with(ctx, module, class, msg.clone(), fields) {
            return Ok(v);
        }
    }
    Err(Raised::Raw(anyhow::anyhow!("{}: {msg}", classes.first().copied().unwrap_or("Std.Exception"))))
}

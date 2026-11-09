//! Context-free text views of values and a stack-first formatting sink.

use super::Value;

/// perf-str-concat-direct: a stack-first `fmt::Write` sink. Up to 64 bytes stay in the
/// inline buffer (every `i64` / `bool` / `char` and the common `f64` texts); longer
/// output (e.g. `1e300` prints 301 digits) spills to a `String`.
pub struct FmtBuf {
    buf: [u8; 64],
    len: usize,
    spill: String,
}

impl FmtBuf {
    #[inline]
    pub fn new() -> Self {
        Self { buf: [0; 64], len: 0, spill: String::new() }
    }
    #[inline]
    pub fn as_str(&self) -> &str {
        if self.spill.is_empty() {
            // SAFETY: only whole `&str` fragments are ever copied into `buf[..len]`.
            unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len]) }
        } else {
            &self.spill
        }
    }
}

impl Default for FmtBuf {
    fn default() -> Self { Self::new() }
}

impl std::fmt::Write for FmtBuf {
    fn write_str(&mut self, s: &str) -> std::fmt::Result {
        if self.spill.is_empty() && self.len + s.len() <= self.buf.len() {
            self.buf[self.len..self.len + s.len()].copy_from_slice(s.as_bytes());
            self.len += s.len();
        } else {
            if self.spill.is_empty() {
                // SAFETY: as in `as_str`.
                self.spill.push_str(unsafe { std::str::from_utf8_unchecked(&self.buf[..self.len]) });
            }
            self.spill.push_str(s);
        }
        Ok(())
    }
}

/// perf-str-concat-direct: [`value_to_str`] as a borrowed view — the text is handed to `f`
/// instead of being returned as an owned `String`. Strings pass through, scalars
/// (`I64` / `F64` / `Bool` / `Char` / `Null`) are formatted into a stack [`FmtBuf`]
/// (no heap allocation); every other variant falls back to `value_to_str`. The text is
/// byte-identical to `value_to_str(v)` (same `Display` impls).
#[inline]
pub fn with_value_str<R>(v: &Value, f: impl FnOnce(&str) -> R) -> R {
    use std::fmt::Write;
    let mut b = FmtBuf::new();
    match v {
        Value::Str(s) => return f(s),
        Value::I64(n) => { let _ = write!(b, "{n}"); }
        Value::F64(x) => { let _ = write!(b, "{x}"); }
        Value::Bool(x) => return f(if *x { "true" } else { "false" }),
        Value::Char(c) => { let mut t = [0u8; 4]; return f(c.encode_utf8(&mut t)); }
        Value::Null => return f("null"),
        other => return f(&value_to_str(other)),
    }
    f(b.as_str())
}

/// Convert a Value to its string representation.
///
/// Exhaustive match: 加新 `Value` variant 时编译期强制覆盖（防止再次出现
/// 像 `Value::Map` 那样"variant 加进 enum 但消费侧忘记更新"的死代码）。
pub fn value_to_str(v: &Value) -> String {
    match v {
        Value::I64(n)  => n.to_string(),
        Value::F64(f)  => f.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Char(c) => c.to_string(),
        Value::Str(s)  => s.to_string(),
        Value::Null    => "null".to_string(),
        Value::Array(rc) => {
            let a = rc.borrow();
            // 值 struct 数组的元素没有 ctx 装不了箱：与装箱 struct 同一个类型名占位。
            let inner: Vec<String> = match a.struct_layout() {
                Some(_) => vec![format!("{}{{...}}", &*a.element_type()); a.len()],
                None => a.iter_boxed().map(|v| value_to_str(&v)).collect(),
            };
            format!("[{}]", inner.join(", "))
        }
        Value::Object(rc) => format!("{}{{...}}", rc.type_desc().name),
        // P1-2 PR 8: callables hold a `FnId`; without a `ctx` the name is not reachable, so
        // the id is shown. The ctx-aware display (`exec::ops::stringify::with_obj_str`, which
        // string interpolation / `Console.WriteLine` / `ToStr` use) prints the name.
        Value::FuncRef(id) => format!("<fn #{}>", id.0),
        Value::Closure(c)                   => format!("<closure #{}>", crate::object::closure_data_of(c).fn_id.0),
        // make-value-copy: PinnedView / Ref are transient-arena handles
        // and `value_to_str` has no `ctx` to resolve the arena payload — placeholders
        // (same as StackObject / StructRef below). These never reach the user-visible
        // stringify path (ToString is an escape sink → the heap form is what materializes).
        Value::PinnedView { .. } => "<pinned view>".to_string(),
        Value::Ref { .. } => "<ref>".to_string(),
        // add-escape-analysis-stack-alloc: stack objects/arrays never reach the
        // user-visible stringify path (ToStr is an escape sink → such objects are
        // heap-allocated, not stack). This arm is defensive: a placeholder that's
        // easy to spot in debugging if an analysis bug ever lets one through
        // (`value_to_str` has no `ctx` to resolve the arena, by design).
        Value::StackObject { .. } => "<stack object>".to_string(),
        Value::StackArray { .. }  => "<stack array>".to_string(),
        // add-struct-value-semantics: struct blob lives in the arena; value_to_str
        // has no ctx to resolve it. ToString on a value struct dispatches through
        // its type's method (VCall), not this raw path — defensive placeholder.
        Value::StructRef { .. } => "<struct value>".to_string(),
        // add-struct-object-boxing → unify Phase 2 R3: 基元装箱盒（int→Std.Int32…）→ 标量字符串
        // （恢复 add-primitive-value-boxing 的 `value_to_str(inner)` 语义——`WriteLine(object)` 把
        // int 实参装箱后须打印 "5" 而非 "Std.Int32{...}"）。boxed struct 的完整 ToString（值格式）由
        // PR2b 合成方法经 VCall 提供；此原始路径给类型名占位（比 StructRef 占位更具体）。
        // make-enum-distinct-type 1.5: an enum box stringifies as its member name
        // (C# `Enum.ToString`) — checked before the scalar arm, which would otherwise
        // print the raw i64. Keeps `Console.WriteLine(Color.Blue)` and
        // `((object)Color.Blue).ToString()` (resolve_vcall) saying the same thing.
        // NB: one `borrow()` for both queries — re-borrowing under the first guard
        // deadlocks (the entry `Mutex` is not reentrant).
        Value::BoxedStruct(gc) => {
            let o = gc.borrow();
            match o.boxed_enum_name() {
                Some(name) => name,
                None => match o.boxed_prim_i64() {
                    Some(n) => n.to_string(),
                    None => format!("{}{{...}}", gc.type_desc().name),
                },
            }
        }
        // make-value-copy: a struct[] element handle — arena-resident, no ctx here;
        // placeholder (ToString on the element dispatches via VCall, not this raw path).
        Value::StructRefHeap { .. } => "<struct value>".to_string(),
    }
}


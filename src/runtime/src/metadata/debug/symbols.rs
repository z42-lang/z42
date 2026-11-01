//! Source locations and variable records decoded from debug sections.

/// An entry in a function's local variable table: register `reg` holds variable `name`.
#[derive(Debug, Clone)]
pub struct LocalVar {
    pub name: String,
    pub reg:  u16,
}

/// An entry in a function's source-line mapping table.
///
/// span-column-propagate (zbc 1.1): `column` carries 1-based
/// source column from `Span.Column`. Value `0` means unknown (legacy
/// hand-rolled IR or pre-1.1 zbc never reach here — reader rejects).
#[derive(Debug, Clone)]
pub struct LineEntry {
    pub block:  u32,
    pub instr:  u32,
    pub line:   u32,
    pub file:   Option<String>,
    pub column: u32,
}

#[derive(Default, Clone, Debug)]
pub struct DbugFuncEntry {
    pub line_table: Vec<crate::metadata::debug::LineEntry>,
    pub local_vars: Vec<crate::metadata::debug::LocalVar>,
}


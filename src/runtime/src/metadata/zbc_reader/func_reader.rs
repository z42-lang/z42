use super::*;

// ── FUNC section ─────────────────────────────────────────────────────────────

pub(super) struct FuncBody {
    pub(super) blocks: Vec<BasicBlock>,
    pub(super) exception_table: Vec<ExceptionEntry>,
    // 1.2 split-debug-symbols: LineTable moved out of FuncBody (FUNC section)
    // into DBUG section. The merged Function.line_table is populated at
    // assembly time from the (optional) DBUG content.
}

// ── Phase 3 S3c (tokenize-ir-and-zbc-bump, 2026-05-09) ────────────────────────
//
// IdMap: maps a v1.0 zbc IR-field token to its FQ name string.
//
//   • token < IMPORT_BASE   → `local_funcs[token]` or `local_classes[token]`
//   • token >= IMPORT_BASE  → `pool[token - IMPORT_BASE]` (cross-zpkg STRS idx)
//   • token == UNRESOLVED   → "<unresolved>" diagnostic placeholder
//
// Pre-1.0 reading was supported in S3a/b transitionally, removed in S3c per
// CLAUDE.md "不为旧版本提供兼容".

pub(super) const IMPORT_BASE_TOKEN: u32 = 0x8000_0000;
pub(super) const UNRESOLVED_TOKEN:  u32 = 0xFFFF_FFFF;

pub(super) struct IdMap<'a> {
    pool: &'a [String],
    local_funcs:   Vec<String>,
    local_classes: Vec<String>,
}

impl<'a> IdMap<'a> {
    pub(super) fn for_v1(pool: &'a [String], local_funcs: Vec<String>, local_classes: Vec<String>) -> Self {
        Self { pool, local_funcs, local_classes }
    }

    pub(super) fn resolve_method(&self, token: u32) -> Result<String> {
        if token == UNRESOLVED_TOKEN {
            return Ok("<unresolved>".to_owned());
        }
        if token >= IMPORT_BASE_TOKEN {
            return pool_str_owned(self.pool, token - IMPORT_BASE_TOKEN);
        }
        self.local_funcs.get(token as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "zbc 1.0 method token {} out of range (local_funcs len {})",
                token, self.local_funcs.len()))
    }

    pub(super) fn resolve_type(&self, token: u32) -> Result<String> {
        if token == UNRESOLVED_TOKEN {
            return Ok("<unresolved>".to_owned());
        }
        if token >= IMPORT_BASE_TOKEN {
            return pool_str_owned(self.pool, token - IMPORT_BASE_TOKEN);
        }
        self.local_classes.get(token as usize)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!(
                "zbc 1.0 type token {} out of range (local_classes len {})",
                token, self.local_classes.len()))
    }
}

pub(super) fn read_func(sec: &[u8], pool: &[String], id_map: &IdMap) -> Result<Vec<FuncBody>> {
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut bodies = Vec::with_capacity(func_count);

    for _ in 0..func_count {
        let _reg_count  = c.read_u16()?;
        let block_count = c.read_u16()? as usize;
        let instr_len   = c.read_u32()? as usize;
        let exc_count   = c.read_u16()? as usize;
        // 1.2 split-debug-symbols: line_count + line_table no longer in FUNC.

        let mut block_offsets = Vec::with_capacity(block_count);
        for _ in 0..block_count {
            block_offsets.push(c.read_u32()? as usize);
        }

        let mut raw_exc = Vec::with_capacity(exc_count);
        for _ in 0..exc_count {
            let try_start  = c.read_u16()?;
            let try_end    = c.read_u16()?;
            let catch_blk  = c.read_u16()?;
            let catch_type = c.read_u32()?;
            let catch_reg  = c.read_u16()?;
            raw_exc.push((try_start, try_end, catch_blk, catch_type, catch_reg));
        }

        let instr_bytes = c.read_bytes(instr_len)?;

        // Decode blocks
        let mut blocks = Vec::with_capacity(block_count);
        for bi in 0..block_count {
            let start = block_offsets[bi];
            let end   = if bi + 1 < block_count { block_offsets[bi + 1] } else { instr_len };
            let label = if bi == 0 { "entry".to_owned() } else { format!("block_{bi}") };
            let (instrs, term) = decode_block(&instr_bytes[start..end], pool, id_map)?;
            blocks.push(BasicBlock { label, instructions: instrs, terminator: term });
        }

        // Resolve exception table block indices to labels
        let exception_table = raw_exc.into_iter().map(|(ts, te, cb, ct, cr)| {
            let try_start  = block_label(ts as usize);
            let try_end    = if (te as usize) < blocks.len() {
                block_label(te as usize)
            } else {
                format!("block_{}", blocks.len())
            };
            let catch_label = block_label(cb as usize);
            let catch_type  = if ct == u32::MAX { None } else {
                pool.get(ct as usize).map(|s| s.clone())
            };
            ExceptionEntry { try_start, try_end, catch_label, catch_type, catch_reg: cr as u32 }
        }).collect();

        bodies.push(FuncBody { blocks, exception_table });
    }
    Ok(bodies)
}

pub(super) fn block_label(idx: usize) -> String {
    if idx == 0 { "entry".to_owned() } else { format!("block_{idx}") }
}

// ── DBUG section (line table + local variable names; 1.2+) ──────────────────

#[derive(Default, Clone, Debug)]
pub struct DbugFuncEntry {
    pub line_table: Vec<crate::metadata::bytecode::LineEntry>,
    pub local_vars: Vec<crate::metadata::bytecode::LocalVar>,
}

pub(super) fn read_dbug(sec: &[u8], pool: &[String]) -> Result<Vec<DbugFuncEntry>> {
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(func_count);

    for _ in 0..func_count {
        // ── Line table ───────────────────────────────────────────────────
        let line_count = c.read_u16()? as usize;
        let mut line_table = Vec::with_capacity(line_count);
        for _ in 0..line_count {
            let blk     = c.read_u16()? as u32;
            let ins     = c.read_u16()? as u32;
            let line    = c.read_u32()?;
            let file_id = c.read_u32()?;
            let column  = c.read_u32()?;
            let file = if file_id == u32::MAX { None } else {
                pool.get(file_id as usize).cloned()
            };
            line_table.push(crate::metadata::bytecode::LineEntry {
                block: blk, instr: ins, line, file, column,
            });
        }

        // ── Local var table ──────────────────────────────────────────────
        let var_count = c.read_u16()? as usize;
        let mut local_vars = Vec::with_capacity(var_count);
        for _ in 0..var_count {
            let name_idx = c.read_u32()? as usize;
            let reg = c.read_u16()?;
            let name = pool.get(name_idx).cloned().unwrap_or_else(|| format!("?{name_idx}"));
            local_vars.push(crate::metadata::bytecode::LocalVar { name, reg });
        }

        result.push(DbugFuncEntry { line_table, local_vars });
    }
    Ok(result)
}

/// jit-type-specialization C2 P0 step 0.4 (zbc 1.8, 2026-05-27): decode the
/// REGT section into one `Box<[IrType]>` per function, indexed by position.
/// Reader is liberal — unknown byte values decode as `IrType::Unknown` (per
/// `IrType::from_u8`), so writer-side variant additions don't break older
/// runtimes.
pub(super) fn read_regt(sec: &[u8]) -> Result<Vec<Box<[crate::metadata::IrType]>>> {
    use crate::metadata::IrType;
    let mut c = Cursor::new(sec);
    let func_count = c.read_u32()? as usize;
    let mut result = Vec::with_capacity(func_count);
    for _ in 0..func_count {
        let reg_count = c.read_u32()? as usize;
        if reg_count == 0 {
            result.push(Box::new([]) as Box<[IrType]>);
            continue;
        }
        let mut types = Vec::with_capacity(reg_count);
        for _ in 0..reg_count {
            types.push(IrType::from_u8(c.read_u8()?));
        }
        result.push(types.into_boxed_slice());
    }
    Ok(result)
}

/// Rebuilds the module-local string pool from the global pool + ConstStr references,
/// and remaps ConstStr.idx from global to local indices in-place.
pub(super) fn rebuild_string_pool(global: &[String], funcs: &mut [Function]) -> Vec<String> {
    let mut seen: HashMap<u32, u32> = HashMap::new();
    let mut local: Vec<String> = Vec::new();

    for func in funcs.iter() {
        for block in &func.blocks {
            for instr in &block.instructions {
                if let Instruction::ConstStr { idx, .. } = instr {
                    if !seen.contains_key(idx) {
                        let s = global.get(*idx as usize).cloned().unwrap_or_default();
                        let local_idx = local.len() as u32;
                        seen.insert(*idx, local_idx);
                        local.push(s);
                    }
                }
            }
        }
    }

    // Remap in-place
    for func in funcs.iter_mut() {
        for block in &mut func.blocks {
            for instr in &mut block.instructions {
                if let Instruction::ConstStr { idx, .. } = instr {
                    if let Some(&new_idx) = seen.get(idx) {
                        *idx = new_idx;
                    }
                }
            }
        }
    }

    local
}

// ── zbc public API ────────────────────────────────────────────────────────────

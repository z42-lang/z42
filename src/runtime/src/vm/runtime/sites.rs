//! Lazy VM-local operand caches. Definitions contain names and source positions only.
use crate::zpkg::ir::{Function, Instruction};
use crate::vm::runtime::ids::tokens::{TypeKeyCell, UNRESOLVED};
use crate::object::{ElemType, TypeArgs};
use rustc_hash::{FxHashMap, FxHashSet};
#[derive(Debug)]
pub(super) struct CodeSites {
    pub indices: Vec<Vec<u32>>,
    pub type_keys: Vec<TypeKeyCell>,
    pub object_args: Vec<TypeArgs>,
    pub array_types: Vec<ElemType>,
    pub catch_keys: Vec<TypeKeyCell>,
    pub legacy_struct_views: FxHashSet<u32>,
}
impl CodeSites {
    pub fn new(function: &Function) -> Self {
        let mut result = Self {
            indices: Vec::new(),
            legacy_struct_views: FxHashSet::default(),
            type_keys: Vec::new(),
            array_types: Vec::new(),
            object_args: Vec::new(),
            catch_keys: (0..function.exception_table().len())
                .map(|_| TypeKeyCell::new())
                .collect(),
        };
        let mut missing_array_reg_type = false;
        for block in &function.blocks {
            let mut row = Vec::with_capacity(block.instructions.len());
            for instr in &block.instructions {
                let index = match instr {
                    Instruction::IsInstance(_) | Instruction::AsCast(_) => {
                        let i = result.type_keys.len();
                        result.type_keys.push(TypeKeyCell::new());
                        i as u32
                    }
                    Instruction::ObjNew(insn) => {
                        let i = result.object_args.len();
                        result.object_args.push(TypeArgs::intern(&insn.type_args));
                        i as u32
                    }
                    Instruction::ArrayNew(insn) => {
                        let i = result.array_types.len();
                        result
                            .array_types
                            .push(ElemType::intern(&insn.element_type));
                        i as u32
                    }
                    Instruction::ArrayNewLit(insn) => {
                        let i = result.array_types.len();
                        result
                            .array_types
                            .push(ElemType::intern(&insn.element_type));
                        i as u32
                    }
                    Instruction::ArrayGet { dst, .. } => {
                        missing_array_reg_type |= function.reg_types.get(*dst as usize).is_none();
                        UNRESOLVED
                    }
                    _ => UNRESOLVED,
                };
                row.push(index);
            }
            result.indices.push(row);
        }
        if missing_array_reg_type {
            result.legacy_struct_views = legacy_struct_views(function);
        }
        result
    }
}

/// Structural uses identify the compiler's concrete struct handles in bytecode without REGT.
/// A Copy can move a handle to another register before the structural use.
fn legacy_struct_views(function: &Function) -> FxHashSet<u32> {
    let mut copies = FxHashMap::<u32, Vec<u32>>::default();
    let mut pending = Vec::new();
    for instr in function.blocks.iter().flat_map(|b| &b.instructions) {
        match instr {
            Instruction::Copy { dst, src } => copies.entry(*dst).or_default().push(*src),
            Instruction::StructCopy { dst, src, .. } => pending.extend([*dst, *src]),
            Instruction::StructFieldGetPrim(insn) => pending.push(insn.base),
            Instruction::StructFieldSetPrim(insn) => pending.push(insn.base),
            _ => {}
        }
    }
    let mut views = FxHashSet::default();
    while let Some(reg) = pending.pop() {
        if views.insert(reg) {
            if let Some(sources) = copies.get(&reg) {
                pending.extend(sources);
            }
        }
    }
    views
}

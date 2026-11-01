//! Lazy VM-local operand caches. Definitions contain names and source positions only.
use crate::metadata::ir::{Function, Instruction};
use crate::metadata::tokens::{TypeKeyCell, UNRESOLVED};
use crate::metadata::types::{ElemType, TypeArgs};
#[derive(Debug)]
pub(super) struct CodeSites {
    pub indices: Vec<Vec<u32>>,
    pub type_keys: Vec<TypeKeyCell>,
    pub object_args: Vec<TypeArgs>,
    pub array_types: Vec<ElemType>,
    pub catch_keys: Vec<TypeKeyCell>,
}
impl CodeSites {
    pub fn new(function: &Function) -> Self {
        let mut result = Self {
            indices: Vec::new(),
            type_keys: Vec::new(),
            array_types: Vec::new(),
            object_args: Vec::new(),
            catch_keys: (0..function.exception_table().len())
                .map(|_| TypeKeyCell::new())
                .collect(),
        };
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
                    _ => UNRESOLVED,
                };
                row.push(index);
            }
            result.indices.push(row);
        }
        result
    }
}

//! A VM's function linkage, sharing an immutable definition with its image.
use super::sites::CodeSites;
use super::SuperInstr;
use crate::metadata::resolver::ResolvedTokens;
use crate::metadata::tokens::{FnIdCell, ForwardCell, TypeKeyCell};
use crate::{metadata::ir, object::TypeDesc};
use std::ops::{Deref, DerefMut};
use std::sync::{Arc, OnceLock};

pub type OwnerInitCell = Arc<OnceLock<Option<Arc<TypeDesc>>>>;
#[derive(Debug)]
pub struct Function {
    definition: Arc<ir::Function>,
    pub resolved: OnceLock<ResolvedTokens>,
    pub owner_init: OwnerInitCell,
    pub id: FnIdCell,
    pub forward: ForwardCell,
    pub fused_tails: Vec<Option<SuperInstr>>,
    sites: OnceLock<Box<CodeSites>>,
}
impl Function {
    pub fn new(definition: ir::Function) -> Self {
        Self::from_definition(Arc::new(definition))
    }
    pub(crate) fn from_definition(definition: Arc<ir::Function>) -> Self {
        Self {
            definition,
            resolved: OnceLock::new(),
            owner_init: Default::default(),
            id: Default::default(),
            forward: Default::default(),
            fused_tails: Vec::new(),
            sites: OnceLock::new(),
        }
    }
    pub fn definition(&self) -> &ir::Function {
        &self.definition
    }
    pub(crate) fn shared_definition(&self) -> Arc<ir::Function> {
        if self.frame_meta.is_some() && self.branch_targets.len() == self.blocks.len() {
            return self.definition.clone();
        }
        let mut definition = (*self.definition).clone();
        definition.prepare();
        Arc::new(definition)
    }
    pub fn type_key(&self, block: usize, instr: usize) -> &TypeKeyCell {
        let sites = self.sites();
        &sites.type_keys[sites.indices[block][instr] as usize]
    }
    pub fn array_type(&self, block: usize, instr: usize) -> crate::object::ElemType {
        let sites = self.sites();
        sites.array_types[sites.indices[block][instr] as usize]
    }
    pub fn object_args(&self, block: usize, instr: usize) -> crate::object::TypeArgs {
        let sites = self.sites();
        sites.object_args[sites.indices[block][instr] as usize]
    }
    pub fn catch_key(&self, entry: usize) -> &TypeKeyCell {
        &self.sites().catch_keys[entry]
    }
    fn sites(&self) -> &CodeSites {
        self.sites
            .get_or_init(|| Box::new(CodeSites::new(&self.definition)))
    }
    pub(crate) fn clear_link_state(&mut self) {
        self.resolved.take();
        if self.owner_init.get().is_some() {
            self.owner_init = Default::default();
        }
        self.id = Default::default();
        self.forward = Default::default();
        self.sites.take();
    }
    /// Rebase string literals without invalidating control-flow or frame metadata.
    pub(crate) fn remap_string_ids(&mut self, offset: u32) {
        if offset == 0 {
            return;
        }
        self.clear_link_state();
        for block in &mut Arc::make_mut(&mut self.definition).blocks {
            for instr in &mut block.instructions {
                if let ir::Instruction::ConstStr { idx, .. } = instr {
                    *idx += offset;
                }
            }
        }
    }
    pub fn prepare(&mut self) {
        let ready = !self.blocks.is_empty() && self.branch_targets.len() == self.blocks.len();
        if !ready {
            Arc::make_mut(&mut self.definition).prepare();
        }
    }
}
impl Deref for Function {
    type Target = ir::Function;
    fn deref(&self) -> &Self::Target {
        &self.definition
    }
}
impl DerefMut for Function {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.clear_link_state();
        self.fused_tails.clear();
        let definition = Arc::make_mut(&mut self.definition);
        definition.block_index.clear();
        definition.branch_targets.clear();
        definition.frame_meta = None;
        definition.max_reg = 0;
        definition
    }
}

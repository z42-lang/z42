//! Load preparation samples these inputs only when computing fused tails.

#[derive(Clone, Copy)]
pub(crate) struct FusionInputs {
    pub(crate) no_fusion: bool,
    pub(crate) no_typed_fusion: bool,
    pub(crate) fusion_debug: bool,
}

pub(crate) type FusionInputSource = fn() -> FusionInputs;

//! Load preparation samples these inputs only when computing fused tails.

#[derive(Clone, Copy)]
pub struct FusionInputs {
    pub no_fusion: bool,
    pub no_typed_fusion: bool,
    pub fusion_debug: bool,
}

pub type FusionInputSource = fn() -> FusionInputs;

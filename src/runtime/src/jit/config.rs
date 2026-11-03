//! Backend-owned policy input. Parsing and defaults belong to the facade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JitConfig {
    pub threshold: u32,
    pub osr_threshold: u32,
    pub interp_tierup: u32,
    pub profile: bool,
    pub debug_promote: bool,
}

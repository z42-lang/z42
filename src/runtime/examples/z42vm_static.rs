//! Static embedding reference; register linked factories, then use the shared host CLI.
fn main() -> anyhow::Result<()> {
    unsafe { z42::host::register_static_component(z42::host::INTERP_COMPONENT, z42_interp::component::z42_interp_init)?; }
    #[cfg(all(feature = "jit", z42_jit_supported))]
    unsafe { z42::host::register_static_component(z42::host::JIT_COMPONENT, z42_jit::component::z42_jit_init)?; }
    z42::host::cli::run_from(std::env::args_os())
}

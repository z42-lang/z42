// The static example links the same process entry used by the dynamic launcher.
unsafe extern "C" {
    fn z42_main_v1() -> i32;
}
#[cfg(all(feature = "mimalloc-alloc", not(feature = "dhat-heap")))]
#[global_allocator]
static ALLOC: mimalloc::MiMalloc = mimalloc::MiMalloc;
#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;
fn main() -> anyhow::Result<()> {
    unsafe { z42::components::abi::register_static_component(z42::components::abi::INTERP_COMPONENT, z42_interp::component::z42_interp_init)?; }
    #[cfg(all(feature = "jit", z42_jit_supported))]
    unsafe { z42::components::abi::register_static_component(z42::components::abi::JIT_COMPONENT, z42_jit::component::z42_jit_init)?; }
    unsafe { z42::components::abi::register_static_component(z42::components::abi::PROFILER_COMPONENT, z42_profiler::component::z42_profiler_init)?; }
    // SAFETY: the linked z42 runtime defines this CLI entry with the C ABI.
    let status = unsafe { z42_main_v1() };
    if status != 0 {
        std::process::exit(status);
    }
    Ok(())
}

//! Proc macro entry points for z42 native interop.
//!
//! C3 wires up `#[z42::methods]` and `z42::module!` to real expansions;
//! `#[derive(Z42Type)]` and `#[z42::trait_impl]` remain stubbed and
//! redirect users to the spec that will land them (C5).

mod builtins_attr;
mod methods_attr;
mod module_macro;
mod shim;
mod signature;
mod util;

use proc_macro::TokenStream;
use quote::quote;

const C5_DERIVE: &str =
    "#[derive(Z42Type)] is reserved for spec C5; in C3 use `#[z42::methods(module = \"...\", name = \"...\")]` on an `impl` block instead";
const C5_TRAIT_IMPL: &str =
    "#[z42::trait_impl] is reserved for spec C5 (source-generator phase); not available in C3";

/// `#[derive(Z42Type)]` — reserved (still stubbed in C3; see message).
#[proc_macro_derive(Z42Type, attributes(z42))]
pub fn derive_z42_type(_input: TokenStream) -> TokenStream {
    quote! { ::core::compile_error!(#C5_DERIVE); }.into()
}

/// `#[z42::methods(module = "...", name = "...")]` on `impl T { ... }`.
#[proc_macro_attribute]
pub fn methods(attr: TokenStream, item: TokenStream) -> TokenStream {
    methods_attr::expand(attr, item)
}

/// `#[builtins]` on an inline `mod { ... }` of `#[builtin(name = "__x", ...)]` functions —
/// the z42 VM's own builtin ABI (typed wrappers + descriptors + `DESCS`). Used by the
/// `z42` crate only; see `builtins_attr`.
#[proc_macro_attribute]
pub fn builtins(attr: TokenStream, item: TokenStream) -> TokenStream {
    builtins_attr::expand(attr, item)
}

/// `#[z42::trait_impl("trait_name")]` — reserved for C5.
#[proc_macro_attribute]
pub fn trait_impl(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    quote! { ::core::compile_error!(#C5_TRAIT_IMPL); }.into()
}

/// `z42::module! { name: "...", types: [T1, T2, ...] }`.
#[proc_macro]
pub fn module(input: TokenStream) -> TokenStream {
    module_macro::expand(input)
}

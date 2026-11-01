//! `#[builtins]` on an inline `mod` — the z42 VM's builtin ABI v2 (runtime-audit P1-8).
//!
//! Every `fn` in the module carrying `#[builtin(...)]` is a VM builtin written as a
//! plain typed Rust function; this macro is its single source of truth. Per function it
//! emits:
//!
//! - an argument-reading wrapper, generic over the argument source, instantiated twice:
//!   `__z42b_regs_<f>` reads the arguments straight out of the caller's register file by
//!   register index (no `Vec` / `SmallVec` of cloned values), `__z42b_slice_<f>` reads a
//!   plain `&[Value]` (callers that hold the arguments as a slice);
//! - a `BuiltinDesc` (name, parameter kinds, return kind, flags, the two entries);
//!
//! and once per module `DESCS: &[BuiltinDesc]`, which the VM's builtin table
//! (`corelib::builtin_table`) concatenates — a builtin is registered by being written
//! here, the table lists modules, not functions.
//!
//! The expansion names VM items by `crate::` paths: the macro is for the `z42` crate.
//!
//! ```ignore
//! #[z42_macros::builtins]
//! pub(crate) mod parse {
//!     use crate::corelib::abi::*;
//!     #[builtin(name = "__int32_parse")]
//!     pub fn int32_parse(s: &str) -> Result<i64, BuiltinError> { … }
//!     #[builtin(name = "__thread_sleep", blocking)]
//!     pub fn sleep(millis: i64) -> Result<(), BuiltinError> { … }
//! }
//! ```
//!
//! Attribute arguments: `name = "__x"` (or a bare string), `blocking`, `pure`,
//! `reenters` (the builtin may run z42 code — a callback, a getter, an initializer —
//! so a folded call of its `[Native]` wrapper keeps the wrapper's frame on the stack),
//! `this = "Member"` (argument 0 is the receiver of instance member `Member`: null →
//! `NullReferenceException` instead of `ArgumentNullException`).
//!
//! A first parameter of type `&VmContext` receives the context (not a z42 argument).
//! `blocking` builtins may not take one: their body runs with the context parked, and
//! their parameter / result types must be plain Rust data (`PlainArg` / `PlainRet`), so
//! the body cannot allocate on the GC heap by construction.
//!
//! A `#[cfg(..)]` on a builtin applies to everything generated for it.

use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{
    Attribute, Expr, ExprLit, FnArg, GenericArgument, Item, ItemFn, ItemMod, Lit, LitStr, Meta,
    Pat, PathArguments, ReturnType, Token, Type,
};

struct BuiltinArgs {
    name: LitStr,
    blocking: bool,
    pure: bool,
    reenters: bool,
    this: Option<LitStr>,
}

fn lit_str(e: &Expr) -> Option<LitStr> {
    match e {
        Expr::Lit(ExprLit { lit: Lit::Str(s), .. }) => Some(s.clone()),
        _ => None,
    }
}

fn parse_builtin_args(attr: &Attribute) -> syn::Result<BuiltinArgs> {
    let metas = match &attr.meta {
        Meta::List(list) => Punctuated::<Expr, Token![,]>::parse_terminated.parse2(list.tokens.clone())?,
        _ => return Err(syn::Error::new(attr.span(), "builtin: expected `#[builtin(name = \"__x\", ...)]`")),
    };
    let mut args = BuiltinArgs {
        name: LitStr::new("", Span::call_site()), blocking: false, pure: false, reenters: false, this: None,
    };
    let mut have_name = false;
    for e in metas {
        match &e {
            Expr::Lit(_) => {
                args.name = lit_str(&e).ok_or_else(|| syn::Error::new(e.span(), "builtin: name must be a string"))?;
                have_name = true;
            }
            Expr::Path(p) if p.path.is_ident("blocking") => args.blocking = true,
            Expr::Path(p) if p.path.is_ident("pure") => args.pure = true,
            Expr::Path(p) if p.path.is_ident("reenters") => args.reenters = true,
            Expr::Assign(a) => {
                let key = match &*a.left {
                    Expr::Path(p) => p.path.get_ident().map(|i| i.to_string()),
                    _ => None,
                };
                let val = lit_str(&a.right)
                    .ok_or_else(|| syn::Error::new(a.right.span(), "builtin: value must be a string literal"))?;
                match key.as_deref() {
                    Some("name") => { args.name = val; have_name = true; }
                    Some("this") => args.this = Some(val),
                    _ => return Err(syn::Error::new(a.left.span(), "builtin: unknown key (name / this)")),
                }
            }
            other => return Err(syn::Error::new(other.span(), "builtin: unknown argument (name / blocking / pure / reenters / this)")),
        }
    }
    if !have_name || args.name.value().is_empty() {
        return Err(syn::Error::new(attr.span(), "builtin: missing `name = \"__x\"`"));
    }
    Ok(args)
}

/// `&VmContext` (any path ending in `VmContext`).
fn is_ctx_type(ty: &Type) -> bool {
    match ty {
        Type::Reference(r) => match &*r.elem {
            Type::Path(p) => p.path.segments.last().is_some_and(|s| s.ident == "VmContext"),
            _ => false,
        },
        _ => false,
    }
}

/// `Some(T)` when `ty` is `Result<T, _>` (any path ending in `Result`).
fn result_ok_type(ty: &Type) -> Option<Type> {
    let Type::Path(p) = ty else { return None };
    let seg = p.path.segments.last()?;
    if seg.ident != "Result" { return None; }
    let PathArguments::AngleBracketed(ab) = &seg.arguments else { return None };
    match ab.args.first()? {
        GenericArgument::Type(t) => Some(t.clone()),
        _ => None,
    }
}

struct Param {
    ident: syn::Ident,
    ty: Type,
}

/// One `#[builtin]` function: the cleaned function plus the generated items.
fn expand_fn(f: &mut ItemFn, args: BuiltinArgs) -> syn::Result<(TokenStream2, syn::Ident, Vec<Attribute>)> {
    let fname = f.sig.ident.clone();
    if !f.sig.generics.params.is_empty() {
        return Err(syn::Error::new(f.sig.generics.span(), "builtin: generic builtins are not supported"));
    }
    let cfgs: Vec<Attribute> = f.attrs.iter().filter(|a| a.path().is_ident("cfg")).cloned().collect();

    let mut takes_ctx = false;
    let mut params: Vec<Param> = Vec::new();
    for (i, input) in f.sig.inputs.iter().enumerate() {
        let FnArg::Typed(pt) = input else {
            return Err(syn::Error::new(input.span(), "builtin: `self` parameters are not supported"));
        };
        if i == 0 && is_ctx_type(&pt.ty) {
            takes_ctx = true;
            continue;
        }
        if is_ctx_type(&pt.ty) {
            return Err(syn::Error::new(pt.ty.span(), "builtin: `&VmContext` must be the first parameter"));
        }
        let ident = match &*pt.pat {
            Pat::Ident(pi) => pi.ident.clone(),
            other => return Err(syn::Error::new(other.span(), "builtin: parameters must be plain identifiers")),
        };
        params.push(Param { ident, ty: (*pt.ty).clone() });
    }
    if args.blocking && takes_ctx {
        return Err(syn::Error::new(
            f.sig.inputs.span(),
            "builtin: a `blocking` builtin runs parked and cannot take `&VmContext` \
             (return `Blocking::new(..)` from a non-blocking builtin when the parked part needs prepared handles)",
        ));
    }

    let (fallible, ret_ty): (bool, Type) = match &f.sig.output {
        ReturnType::Default => (false, syn::parse_quote!(())),
        ReturnType::Type(_, t) => match result_ok_type(t) {
            Some(ok) => (true, ok),
            None => (false, (**t).clone()),
        },
    };

    let name = &args.name;
    let n = params.len();
    let abi = quote!(crate::corelib::abi);
    let inner = format_ident!("__z42b_call_{}", fname);
    let regs_fn = format_ident!("__z42b_regs_{}", fname);
    let slice_fn = format_ident!("__z42b_slice_{}", fname);
    let desc = format_ident!("__Z42B_DESC_{}", fname.to_string().to_uppercase());
    let slot = format_ident!("__Z42B_SLOT_{}", fname.to_string().to_uppercase());

    let reads = params.iter().enumerate().map(|(i, p)| {
        let id = &p.ident;
        let ty = &p.ty;
        let pname = id.to_string();
        let receiver = match (&args.this, i) {
            (Some(m), 0) => quote!(::core::option::Option::Some(#m)),
            _ => quote!(::core::option::Option::None),
        };
        quote! {
            let #id = <#ty as #abi::FromArg>::from_arg(
                __a.arg(#i),
                &#abi::ArgAt { builtin: #name, index: #i, param: #pname, receiver: #receiver },
            )?;
        }
    });
    let call_args = params.iter().map(|p| &p.ident);
    let ctx_arg = if takes_ctx { quote!(__ctx,) } else { quote!() };
    let call = if args.blocking {
        quote! {
            let __r = {
                let __park = crate::gc::NativeParkGuard::enter(__ctx);
                #fname(#(#call_args),*)
            };
        }
    } else {
        quote! { let __r = #fname(#ctx_arg #(#call_args),*); }
    };
    let finish = if fallible {
        quote! { #abi::IntoRet::into_ret(__r?, __ctx) }
    } else {
        quote! { #abi::IntoRet::into_ret(__r, __ctx) }
    };
    let plain_checks = if args.blocking {
        let tys = params.iter().map(|p| &p.ty);
        quote! {
            #(#cfgs)*
            const _: fn() = || {
                fn __plain_arg<T: #abi::PlainArg>() {}
                fn __plain_ret<T: #abi::PlainRet>() {}
                #( __plain_arg::<#tys>(); )*
                __plain_ret::<#ret_ty>();
            };
        }
    } else {
        quote!()
    };
    let kinds = params.iter().map(|p| {
        let ty = &p.ty;
        quote!(<#ty as #abi::FromArg>::KIND)
    });
    let flags = {
        let b = args.blocking;
        let p = args.pure;
        let r = args.reenters;
        if b && r {
            return Err(syn::Error::new(name.span(), "builtin: a `blocking` builtin runs parked and cannot re-enter z42 code"));
        }
        quote!(#abi::BuiltinFlags { blocking: #b, pure: #p, reenters: #r })
    };

    let out = quote! {
        #(#cfgs)*
        #[doc(hidden)]
        #[inline(always)]
        #[allow(non_snake_case, clippy::needless_question_mark, unused_variables)]
        fn #inner<A: #abi::ArgSource + ?Sized>(
            __ctx: &crate::vm_context::VmContext, __a: &A,
        ) -> #abi::CallResult {
            if __a.argc() != #n {
                return ::core::result::Result::Err(#abi::BuiltinError::arity(#name, #n, __a.argc()).into());
            }
            #(#reads)*
            #call
            #finish
        }
        #(#cfgs)*
        #[doc(hidden)]
        fn #regs_fn(
            __ctx: &crate::vm_context::VmContext,
            this: ::core::option::Option<crate::object::Value>,
            regs: &[crate::object::Value],
            idx: &[u32],
        ) -> #abi::CallResult {
            #inner(__ctx, &#abi::RegArgs::new(this, regs, idx))
        }
        #(#cfgs)*
        #[doc(hidden)]
        fn #slice_fn(
            __ctx: &crate::vm_context::VmContext, args: &[crate::object::Value],
        ) -> #abi::CallResult {
            #inner(__ctx, args)
        }
        #(#cfgs)*
        #[doc(hidden)]
        pub(crate) const #desc: #abi::BuiltinDesc = #abi::BuiltinDesc {
            name: #name,
            params: &[#(#kinds),*],
            ret: <#ret_ty as #abi::IntoRet>::KIND,
            flags: #flags,
            call_regs: #regs_fn,
            call_slice: #slice_fn,
        };
        #(#cfgs)*
        #[doc(hidden)]
        const #slot: &[#abi::BuiltinDesc] = &[#desc];
        #plain_checks
    };
    Ok((out, slot, cfgs))
}

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    match expand2(attr.into(), item.into()) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn expand2(attr: TokenStream2, item: TokenStream2) -> syn::Result<TokenStream2> {
    if !attr.is_empty() {
        return Err(syn::Error::new(attr.span(), "builtins: takes no arguments"));
    }
    let mut module: ItemMod = syn::parse2(item)?;
    let Some((_, items)) = module.content.as_mut() else {
        return Err(syn::Error::new(module.span(), "builtins: needs an inline module (`mod x { ... }`)"));
    };
    let abi = quote!(crate::corelib::abi);
    let mut generated = Vec::new();
    let mut slots = Vec::new();
    for it in items.iter_mut() {
        let Item::Fn(f) = it else { continue };
        let Some(pos) = f.attrs.iter().position(|a| a.path().is_ident("builtin")) else { continue };
        let attr = f.attrs.remove(pos);
        let args = parse_builtin_args(&attr)?;
        let (ts, slot, cfgs) = expand_fn(f, args)?;
        generated.push(ts);
        slots.push((slot, cfgs));
    }
    // Per-builtin one-element slots (`&[]` when its `cfg` is off) concatenated into
    // `DESCS` at compile time, so a `cfg`-gated builtin simply drops out.
    let slot_refs: Vec<TokenStream2> = slots.iter().map(|(s, cfgs)| {
        if cfgs.is_empty() { return quote!(#s); }
        let preds: Vec<TokenStream2> = cfgs.iter().filter_map(|a| match &a.meta {
            Meta::List(l) => Some(l.tokens.clone()),
            _ => None,
        }).collect();
        quote!({
            #[cfg(all(#(#preds),*))] const __S: &[#abi::BuiltinDesc] = #s;
            #[cfg(not(all(#(#preds),*)))] const __S: &[#abi::BuiltinDesc] = &[];
            __S
        })
    }).collect();
    let tail: TokenStream2 = quote! {
        #(#generated)*
        #[doc(hidden)]
        const __Z42B_PARTS: &[&[#abi::BuiltinDesc]] = &[#(#slot_refs),*];
        #[doc(hidden)]
        const __Z42B_N: usize = #abi::descs_len(__Z42B_PARTS);
        #[doc(hidden)]
        const __Z42B_ARR: [#abi::BuiltinDesc; __Z42B_N] = #abi::concat_descs(__Z42B_PARTS);
        /// The builtins this module defines, for the VM's builtin table.
        pub(crate) const DESCS: &[#abi::BuiltinDesc] = &__Z42B_ARR;
    };
    let tail_items: syn::File = syn::parse2(tail)?;
    items.extend(tail_items.items);
    Ok(quote!(#module))
}

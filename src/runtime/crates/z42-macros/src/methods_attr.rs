//! `#[z42::methods(module = "...", name = "...")]` on `impl T { ... }`.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::parse::Parser;
use syn::{
    punctuated::Punctuated, spanned::Spanned, Expr, ExprLit, ImplItem, ImplItemFn,
    ItemImpl, Lit, LitStr, Meta, Token,
};

use crate::shim;
use crate::signature;
use crate::util;

/// Parse `module = "...", name = "..."` from the attribute argument list.
struct MethodsArgs {
    module: LitStr,
    name: LitStr,
}

fn parse_args(attr: TokenStream) -> Result<MethodsArgs, syn::Error> {
    let parser = Punctuated::<Meta, Token![,]>::parse_terminated;
    let metas = parser.parse(attr).map_err(|e| {
        syn::Error::new(
            e.span(),
            format!("z42::methods: failed to parse attribute args: {e}"),
        )
    })?;

    let mut module: Option<LitStr> = None;
    let mut name: Option<LitStr> = None;

    for meta in metas {
        let nv = match meta {
            Meta::NameValue(nv) => nv,
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "z42::methods: expected `key = \"value\"` form",
                ));
            }
        };
        let ident = nv.path.get_ident().ok_or_else(|| {
            syn::Error::new(nv.path.span(), "z42::methods: expected identifier key")
        })?;
        let value = match &nv.value {
            Expr::Lit(ExprLit {
                lit: Lit::Str(s), ..
            }) => s.clone(),
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "z42::methods: attribute value must be a string literal",
                ));
            }
        };
        match ident.to_string().as_str() {
            "module" => module = Some(value),
            "name" => name = Some(value),
            other => {
                return Err(syn::Error::new(
                    nv.path.span(),
                    format!("z42::methods: unknown attribute `{other}`"),
                ));
            }
        }
    }

    let module = module.ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            "z42::methods: missing `module = \"...\"` attribute",
        )
    })?;
    let name = name.ok_or_else(|| {
        syn::Error::new(
            Span::call_site(),
            "z42::methods: missing `name = \"...\"` attribute",
        )
    })?;
    Ok(MethodsArgs { module, name })
}

pub fn expand(attr: TokenStream, item: TokenStream) -> TokenStream {
    let args = match parse_args(attr) {
        Ok(a) => a,
        Err(e) => return util::err_to_tokens(e),
    };

    let mut input = match syn::parse::<ItemImpl>(item) {
        Ok(i) => i,
        Err(e) => return util::err_to_tokens(e),
    };

    if let Err(e) = validate_impl(&input) {
        return util::err_to_tokens(e);
    }

    // Resolve the impl target type as an Ident (only `impl T { ... }` for
    // simple types; reject `impl<T> Foo<T>` etc.).
    let type_ident = match &*input.self_ty {
        syn::Type::Path(p) if p.qself.is_none() => match p.path.get_ident() {
            Some(id) => id.clone(),
            None => {
                return util::err_to_tokens(syn::Error::new(
                    p.span(),
                    "z42::methods: impl target must be a single identifier (no generics in C3)",
                ))
            }
        },
        other => {
            return util::err_to_tokens(syn::Error::new(
                other.span(),
                "z42::methods: impl target must be a simple type identifier",
            ))
        }
    };

    let module_str = args.module.value();
    let name_str = args.name.value();

    if let Err(e) = util::validate_module_name(&module_str, args.module.span()) {
        return util::err_to_tokens(e);
    }

    // Walk methods, building shim TokenStreams and method-desc tuples.
    let mut shims: Vec<proc_macro2::TokenStream> = Vec::new();
    let mut method_desc_entries: Vec<proc_macro2::TokenStream> = Vec::new();

    for it in input.items.iter() {
        if let ImplItem::Fn(m) = it {
            if let Err(e) = check_method(m) {
                return util::err_to_tokens(e);
            }
            let (recv, params, ret) = match signature::parse_method_signature(&m.sig) {
                Ok(t) => t,
                Err(e) => return util::err_to_tokens(e),
            };

            let sig_str = signature::render_signature(recv.as_ref(), &params, &ret);
            let shim_ts = shim::render_shim(&type_ident, m, recv.as_ref(), &params, &ret);
            let shim_id = shim::shim_ident(&type_ident, &m.sig.ident);
            let method_lit = m.sig.ident.to_string();

            shims.push(shim_ts);

            let name_c = util::c_string_literal(&method_lit);
            let sig_c = util::c_string_literal(&sig_str);
            let flags = if recv.is_some() {
                quote! { ::z42_abi::Z42_METHOD_FLAG_VIRTUAL }
            } else {
                quote! { ::z42_abi::Z42_METHOD_FLAG_STATIC }
            };

            method_desc_entries.push(quote! {
                ::z42_abi::Z42MethodDesc {
                    name:      #name_c,
                    signature: #sig_c,
                    fn_ptr:    #shim_id as *mut ::core::ffi::c_void,
                    flags:     #flags,
                    reserved:  0,
                }
            });
        }
    }

    let methods_static = util::private_ident("", &type_ident, "_METHODS");
    let desc_static = util::private_ident("", &type_ident, "_DESC");
    let alloc_fn = format_ident!("__z42_alloc_{}", type_ident);
    let dealloc_fn = format_ident!("__z42_dealloc_{}", type_ident);
    let ctor_fn = format_ident!("__z42_ctor_{}", type_ident);
    let dtor_fn = format_ident!("__z42_dtor_{}", type_ident);

    let module_c = util::c_string_literal(&module_str);
    let name_c = util::c_string_literal(&name_str);

    let n = method_desc_entries.len();
    let methods_array = if n == 0 {
        quote! {
            #[allow(non_upper_case_globals)]
            const #methods_static: [::z42_abi::Z42MethodDesc; 0] = [];
        }
    } else {
        quote! {
            #[allow(non_upper_case_globals)]
            static #methods_static: [::z42_abi::Z42MethodDesc; #n] = [
                #(#method_desc_entries),*
            ];
        }
    };

    let methods_ptr_expr = if n == 0 {
        quote! { ::core::ptr::null() }
    } else {
        quote! { #methods_static.as_ptr() }
    };

    strip_methods_attrs(&mut input);

    let expanded = quote! {
        #input

        #(#shims)*

        #methods_array

        // Lifecycle
        #[allow(non_snake_case, non_upper_case_globals)]
        unsafe extern "C" fn #alloc_fn() -> *mut ::core::ffi::c_void {
            ::std::boxed::Box::into_raw(::std::boxed::Box::new(
                ::core::mem::MaybeUninit::<#type_ident>::uninit()
            )) as *mut ::core::ffi::c_void
        }
        #[allow(non_snake_case)]
        unsafe extern "C" fn #dealloc_fn(p: *mut ::core::ffi::c_void) {
            if !p.is_null() {
                drop(unsafe { ::std::boxed::Box::from_raw(p as *mut #type_ident) });
            }
        }
        #[allow(non_snake_case)]
        unsafe extern "C" fn #ctor_fn(p: *mut ::core::ffi::c_void, _args: *const ::z42_abi::Z42Args) {
            unsafe {
                ::core::ptr::write(p as *mut #type_ident, <#type_ident as ::core::default::Default>::default());
            }
        }
        #[allow(non_snake_case)]
        unsafe extern "C" fn #dtor_fn(p: *mut ::core::ffi::c_void) {
            if !p.is_null() {
                unsafe { ::core::ptr::drop_in_place(p as *mut #type_ident); }
            }
        }

        #[allow(non_upper_case_globals)]
        static #desc_static: ::z42_abi::Z42TypeDescriptor_v1 = ::z42_abi::Z42TypeDescriptor_v1 {
            abi_version:    ::z42_abi::Z42_ABI_VERSION,
            flags:          ::z42_abi::Z42_TYPE_FLAG_SEALED,
            module_name:    #module_c,
            type_name:      #name_c,
            instance_size:  ::core::mem::size_of::<#type_ident>(),
            instance_align: ::core::mem::align_of::<#type_ident>(),
            alloc:          ::core::option::Option::Some(#alloc_fn),
            ctor:           ::core::option::Option::Some(#ctor_fn),
            dtor:           ::core::option::Option::Some(#dtor_fn),
            dealloc:        ::core::option::Option::Some(#dealloc_fn),
            retain:         ::core::option::Option::None,
            release:        ::core::option::Option::None,
            method_count:   #n,
            methods:        #methods_ptr_expr,
            field_count:    0,
            fields:         ::core::ptr::null(),
            trait_impl_count: 0,
            trait_impls:    ::core::ptr::null(),
        };

        impl ::z42_rs::traits::Z42Type for #type_ident {
            const MODULE: &'static str = #module_str;
            const NAME:   &'static str = #name_str;
            fn descriptor() -> *const ::z42_abi::Z42TypeDescriptor_v1 {
                &#desc_static
            }
        }
    };

    expanded.into()
}

fn validate_impl(input: &ItemImpl) -> Result<(), syn::Error> {
    if input.generics.params.iter().count() > 0 {
        return Err(syn::Error::new(
            input.generics.span(),
            "z42::methods does not support generics in C3",
        ));
    }
    if input.unsafety.is_some() {
        return Err(syn::Error::new(
            input.span(),
            "z42::methods cannot wrap an `unsafe impl`",
        ));
    }
    if input.trait_.is_some() {
        return Err(syn::Error::new(
            input.span(),
            "z42::methods works on inherent `impl T { ... }` only (use `#[z42::trait_impl]` once it lands in C5)",
        ));
    }
    Ok(())
}

fn check_method(m: &ImplItemFn) -> Result<(), syn::Error> {
    if m.sig.asyncness.is_some() {
        return Err(syn::Error::new(
            m.sig.span(),
            "z42::methods does not support async fns in C3",
        ));
    }
    if matches!(m.sig.safety, syn::Safety::Unsafe(_)) {
        return Err(syn::Error::new(
            m.sig.span(),
            "z42::methods does not support `unsafe fn` in C3",
        ));
    }
    if !m.sig.generics.params.is_empty() {
        return Err(syn::Error::new(
            m.sig.generics.span(),
            "z42::methods does not support generic methods in C3",
        ));
    }
    Ok(())
}

/// Methods inside the `impl` block may carry `#[z42(...)]` per-method
/// attributes in future; for C3 we just leave them through. Hook is here
/// in case future work needs cleanup.
fn strip_methods_attrs(_input: &mut ItemImpl) {}

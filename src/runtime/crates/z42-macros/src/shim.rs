//! Generate one `extern "C"` shim per user method.
//!
//! Each shim:
//! - converts raw pointer / primitive args into Rust types
//! - calls the user method inside `std::panic::catch_unwind`
//! - on panic forwards a message via `z42_rs::native_helpers::set_panic`
//!   and returns `Default::default()` for the return type

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Ident, ImplItemFn, Type};

use crate::signature::AbiTy;

/// Produce the shim function token stream for one user method.
pub(crate) fn render_shim(
    type_ident: &Ident,
    method: &ImplItemFn,
    receiver: Option<&AbiTy>,
    params: &[AbiTy],
    ret: &AbiTy,
) -> TokenStream {
    let method_ident = &method.sig.ident;
    let shim_ident = format_ident!("__shim_{}_{}", type_ident, method_ident);

    // Build C-side parameter list (TokenStream of `name: TypeTokens`).
    let mut shim_params: Vec<TokenStream> = Vec::new();
    let mut call_args: Vec<TokenStream> = Vec::new();

    let receiver_kind = receiver.cloned();
    if let Some(rcv) = &receiver_kind {
        let _ = rcv; // we always treat as `*mut Type` in shim signature
        shim_params.push(quote! { __self_ptr: *mut #type_ident });
        // Receiver consumed below depending on &self / &mut self
    }

    // Filter syn args matching params (skip the receiver if any)
    let typed_inputs: Vec<&syn::PatType> = method
        .sig
        .inputs
        .iter()
        .filter_map(|a| match a {
            syn::FnArg::Typed(pt) => Some(pt),
            _ => None,
        })
        .collect();

    for (i, (abi, pt)) in params.iter().zip(typed_inputs.iter()).enumerate() {
        let arg_name = format_ident!("__arg{}", i);
        let ty_tokens = abi_param_type(abi, &pt.ty);
        shim_params.push(quote! { #arg_name: #ty_tokens });
        call_args.push(quote! { #arg_name });
    }

    let return_ty = match ret {
        AbiTy::Void => quote! { () },
        AbiTy::Bool => quote! { bool },
        AbiTy::I8 => quote! { i8 }, AbiTy::I16 => quote! { i16 },
        AbiTy::I32 => quote! { i32 }, AbiTy::I64 => quote! { i64 },
        AbiTy::U8 => quote! { u8 }, AbiTy::U16 => quote! { u16 },
        AbiTy::U32 => quote! { u32 }, AbiTy::U64 => quote! { u64 },
        AbiTy::F32 => quote! { f32 }, AbiTy::F64 => quote! { f64 },
        AbiTy::Ptr | AbiTy::SelfRef => quote! { *mut ::core::ffi::c_void },
    };

    let panic_msg = format!("{type_ident}::{method_ident} panicked");

    let body_call = match (&receiver_kind, &method.sig.output) {
        (Some(_), _) => {
            // &self vs &mut self — pick deref form by looking at receiver token in source
            let recv_node = method.sig.receiver().expect("receiver present");
            let mutability = matches!(recv_node.kind, syn::ReceiverKind::Reference(_, _, Some(_)));
            if mutability {
                quote! {
                    let __recv = unsafe { &mut *__self_ptr };
                    #type_ident::#method_ident(__recv, #(#call_args),*)
                }
            } else {
                quote! {
                    let __recv = unsafe { &*__self_ptr };
                    #type_ident::#method_ident(__recv, #(#call_args),*)
                }
            }
        }
        (None, _) => {
            quote! { #type_ident::#method_ident(#(#call_args),*) }
        }
    };

    let void_return = matches!(ret, AbiTy::Void);
    let ok_branch = if void_return {
        quote! { Ok(_) => () }
    } else {
        quote! { Ok(__v) => __v }
    };
    let err_default = if void_return {
        quote! { () }
    } else {
        quote! { <#return_ty as ::core::default::Default>::default() }
    };

    let return_clause = if void_return {
        quote! { -> () }
    } else {
        quote! { -> #return_ty }
    };

    quote! {
        #[unsafe(no_mangle)]
        #[allow(non_snake_case)]
        unsafe extern "C" fn #shim_ident(#(#shim_params),*) #return_clause {
            let __r = ::std::panic::catch_unwind(::std::panic::AssertUnwindSafe(|| {
                #body_call
            }));
            match __r {
                #ok_branch,
                Err(_) => {
                    ::z42_rs::native_helpers::set_panic(#panic_msg);
                    #err_default
                }
            }
        }
    }
}

fn abi_param_type(abi: &AbiTy, _user_ty: &Type) -> TokenStream {
    match abi {
        AbiTy::I8 => quote! { i8 },
        AbiTy::I16 => quote! { i16 },
        AbiTy::I32 => quote! { i32 },
        AbiTy::I64 => quote! { i64 },
        AbiTy::U8 => quote! { u8 },
        AbiTy::U16 => quote! { u16 },
        AbiTy::U32 => quote! { u32 },
        AbiTy::U64 => quote! { u64 },
        AbiTy::F32 => quote! { f32 },
        AbiTy::F64 => quote! { f64 },
        AbiTy::Bool => quote! { bool },
        AbiTy::Ptr => quote! { *mut ::core::ffi::c_void },
        AbiTy::SelfRef => quote! { *mut ::core::ffi::c_void },
        AbiTy::Void => quote! { () },
    }
}

/// Identifier used in the static method-table array element.
pub(crate) fn shim_ident(type_ident: &Ident, method: &Ident) -> Ident {
    format_ident!("__shim_{}_{}", type_ident, method)
}


//! Map Rust types in `impl` method signatures to z42 ABI types.
//!
//! The C2 dispatch layer parses these strings back into `SigType`s, so the
//! string format here must round-trip with `dispatch::parse_signature`.
//! Unsupported types produce a clean `syn::Error` pointing at the offending
//! token; callers convert to `compile_error!` via `err_to_tokens`.

use syn::{spanned::Spanned, FnArg, Pat, Receiver, ReceiverKind, ReturnType, Type, TypePath, TypePtr, TypeReference};

/// One operand position in an ABI method signature.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AbiTy {
    Void,
    I8, I16, I32, I64,
    U8, U16, U32, U64,
    F32, F64,
    Bool,
    /// `*const T` / `*mut T` — element type erased at the ABI layer.
    Ptr,
    /// `&self` / `&mut self` / `*const Self` / `*mut Self`.
    SelfRef,
}

impl AbiTy {
    pub fn render(&self) -> &'static str {
        match self {
            AbiTy::Void => "()",
            AbiTy::I8   => "i8",
            AbiTy::I16  => "i16",
            AbiTy::I32  => "i32",
            AbiTy::I64  => "i64",
            AbiTy::U8   => "u8",
            AbiTy::U16  => "u16",
            AbiTy::U32  => "u32",
            AbiTy::U64  => "u64",
            AbiTy::F32  => "f32",
            AbiTy::F64  => "f64",
            AbiTy::Bool => "bool",
            AbiTy::Ptr  => "*mut void",
            AbiTy::SelfRef => "*mut Self",
        }
    }
}

/// Render a parameter list + return type into the `(P1, P2, ...) -> R`
/// signature string the dispatch layer parses.
pub(crate) fn render_signature(receiver: Option<&AbiTy>, params: &[AbiTy], ret: &AbiTy) -> String {
    let mut s = String::from("(");
    let mut first = true;
    if let Some(r) = receiver {
        s.push_str(match r {
            AbiTy::SelfRef => "*mut Self",
            other => other.render(),
        });
        first = false;
    }
    for p in params {
        if !first { s.push_str(", "); }
        s.push_str(p.render());
        first = false;
    }
    s.push_str(") -> ");
    s.push_str(ret.render());
    s
}

/// Walk a function's signature; produce the receiver (if any) plus
/// parameter and return types.
pub(crate) fn parse_method_signature(
    sig: &syn::Signature,
) -> Result<(Option<AbiTy>, Vec<AbiTy>, AbiTy), syn::Error> {
    let mut receiver = None;
    let mut params = Vec::new();

    for input in sig.inputs.iter() {
        match input {
            FnArg::Receiver(r) => receiver = Some(parse_receiver(r)?),
            FnArg::Typed(p) => {
                if matches!(*p.pat, Pat::Wild(_)) {
                    // `_: T` — fine, just skip the name
                }
                params.push(parse_type(&p.ty)?);
            }
        }
    }

    let ret = match &sig.output {
        ReturnType::Default => AbiTy::Void,
        ReturnType::Type(_, ty) => parse_type(ty)?,
    };

    Ok((receiver, params, ret))
}

fn parse_receiver(r: &Receiver) -> Result<AbiTy, syn::Error> {
    if matches!(r.kind, ReceiverKind::Reference(..)) {
        // `&self` or `&mut self`
        return Ok(AbiTy::SelfRef);
    }
    Err(syn::Error::new(
        r.span(),
        "z42::methods does not accept `self` by value (use `&self` or `&mut self`)",
    ))
}

fn parse_type(ty: &Type) -> Result<AbiTy, syn::Error> {
    match ty {
        Type::Path(TypePath { qself: None, path, .. }) => parse_path(path),
        Type::Tuple(t) if t.elems.is_empty() => Ok(AbiTy::Void),
        Type::Ptr(TypePtr { elem, .. }) => {
            // *const Self / *mut Self → SelfRef
            if let Type::Path(TypePath { qself: None, path, .. }) = elem.as_ref() {
                if path.is_ident("Self") {
                    return Ok(AbiTy::SelfRef);
                }
            }
            Ok(AbiTy::Ptr)
        }
        Type::Reference(TypeReference { elem, .. }) => {
            // &Self / &mut Self → SelfRef; other refs → reject (pinned in C4)
            if let Type::Path(TypePath { qself: None, path, .. }) = elem.as_ref() {
                if path.is_ident("Self") {
                    return Ok(AbiTy::SelfRef);
                }
            }
            Err(syn::Error::new(
                ty.span(),
                "Rust references other than `&Self` / `&mut Self` are not yet supported (pinned types arrive in spec C4)",
            ))
        }
        _ => Err(syn::Error::new(
            ty.span(),
            "type not supported by z42::methods (only blittable primitives, raw pointers, and Self refs in C3)",
        )),
    }
}

fn parse_path(path: &syn::Path) -> Result<AbiTy, syn::Error> {
    let span = path.span();
    if path.is_ident("Self") {
        return Ok(AbiTy::SelfRef);
    }
    let Some(ident) = path.get_ident() else {
        return Err(syn::Error::new(
            span,
            "complex type paths are not supported in C3",
        ));
    };
    let s = ident.to_string();
    Ok(match s.as_str() {
        "i8" => AbiTy::I8,
        "i16" => AbiTy::I16,
        "i32" => AbiTy::I32,
        "i64" => AbiTy::I64,
        "isize" => AbiTy::I64,
        "u8" => AbiTy::U8,
        "u16" => AbiTy::U16,
        "u32" => AbiTy::U32,
        "u64" => AbiTy::U64,
        "usize" => AbiTy::U64,
        "f32" => AbiTy::F32,
        "f64" => AbiTy::F64,
        "bool" => AbiTy::Bool,
        "String" | "str" => {
            return Err(syn::Error::new(
                span,
                "String / &str not supported in C3 (pinned borrow lands in spec C4)",
            ));
        }
        "Vec" => {
            return Err(syn::Error::new(
                span,
                "Vec not supported in C3 (pinned borrow lands in spec C4)",
            ));
        }
        _ => {
            return Err(syn::Error::new(
                span,
                format!("type {s} not in the C3 blittable subset"),
            ));
        }
    })
}

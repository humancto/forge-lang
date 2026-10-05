//! `#[forge_fn]` — the attribute behind `forge_plugin::forge_fn`.
//!
//! Use it through the `forge-plugin` crate; the generated code refers to
//! `::forge_plugin`, so the plugin crate must depend on it under that name.
//!
//! For `fn add(a: i64, b: i64) -> i64` it keeps the function unchanged and
//! adds:
//!
//! * a hidden brace struct `add {}` — it lives in the *type* namespace, so it
//!   does not clash with the function — implementing
//!   `forge_plugin::ForgeFunctionDef`, whose `DEF` is the ABI table entry
//!   `forge_plugin::export!` collects;
//! * an `extern "C"` wrapper that converts each argument with `FromForge`
//!   (errors name the function and parameter), calls the function, and
//!   converts the result with `IntoForgeResult`. Panics are caught by
//!   `forge_plugin::__private::invoke` and never unwind into the host.

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{format_ident, quote};
use syn::{parse_macro_input, spanned::Spanned, FnArg, ItemFn, LitStr, Pat};

/// Export a free function to Forge. See the `forge-plugin` crate docs.
///
/// Optional: `#[forge_fn(name = "forge_name")]` to export under another name.
#[proc_macro_attribute]
pub fn forge_fn(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut export_name: Option<LitStr> = None;
    let attr_parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("name") {
            export_name = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("unsupported forge_fn option; expected `name = \"...\"`"))
        }
    });
    parse_macro_input!(attr with attr_parser);
    let func = parse_macro_input!(item as ItemFn);
    match expand(func, export_name) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn is_identifier(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn expand(func: ItemFn, export_name: Option<LitStr>) -> syn::Result<proc_macro2::TokenStream> {
    let sig = &func.sig;
    if let Some(asyncness) = &sig.asyncness {
        return Err(syn::Error::new(
            asyncness.span(),
            "#[forge_fn] functions cannot be async",
        ));
    }
    if !sig.generics.params.is_empty() || sig.generics.where_clause.is_some() {
        return Err(syn::Error::new(
            sig.generics.span(),
            "#[forge_fn] functions cannot be generic",
        ));
    }
    if let Some(variadic) = &sig.variadic {
        return Err(syn::Error::new(
            variadic.span(),
            "#[forge_fn] functions cannot be C-variadic",
        ));
    }
    if sig.abi.is_some() {
        return Err(syn::Error::new(
            sig.abi.span(),
            "write a plain Rust function; #[forge_fn] generates the extern \"C\" wrapper",
        ));
    }

    let ident = &sig.ident;
    let rust_name = ident.to_string();
    let rust_name = rust_name
        .strip_prefix("r#")
        .unwrap_or(&rust_name)
        .to_string();
    let forge_name = match &export_name {
        Some(lit) => {
            let value = lit.value();
            if !is_identifier(&value) {
                return Err(syn::Error::new(
                    lit.span(),
                    "the exported name must be an identifier",
                ));
            }
            value
        }
        None => rust_name.clone(),
    };

    let mut conversions = Vec::new();
    let mut call_args = Vec::new();
    for (index, input) in sig.inputs.iter().enumerate() {
        let typed = match input {
            FnArg::Typed(typed) => typed,
            FnArg::Receiver(receiver) => {
                return Err(syn::Error::new(
                    receiver.span(),
                    "#[forge_fn] must be a free function (no self)",
                ))
            }
        };
        let param_name = match &*typed.pat {
            Pat::Ident(pat) => pat.ident.to_string(),
            Pat::Wild(_) => format!("_{}", index + 1),
            other => {
                return Err(syn::Error::new(
                    other.span(),
                    "#[forge_fn] parameters must be plain names",
                ))
            }
        };
        let ty = &typed.ty;
        let local = format_ident!("__forge_arg_{}", index);
        let position = index + 1;
        conversions.push(quote! {
            let #local: #ty = ::forge_plugin::__private::arg::<#ty>(
                &mut __forge_args, #forge_name, #position, #param_name,
            )?;
        });
        call_args.push(local);
    }
    let arity = i32::try_from(sig.inputs.len())
        .map_err(|_| syn::Error::new(sig.inputs.span(), "too many parameters"))?;
    let arity_usize = sig.inputs.len();
    // `-> T` or nothing (unit): `IntoForgeResult` handles both, and
    // `Result<T, E: Display>` becomes a Forge error.

    let vis = &func.vis;
    let marker = ident;
    let c_name = LitStr::new(&format!("{}\0", forge_name), Span::call_site());

    Ok(quote! {
        #func

        #[doc(hidden)]
        #[allow(non_camel_case_types, dead_code)]
        #vis struct #marker {}

        const _: () = {
            unsafe extern "C" fn __forge_call(
                args: *const ::forge_plugin::abi::ForgeValue,
                argc: usize,
                out: *mut ::forge_plugin::abi::ForgeValue,
            ) -> i32 {
                ::forge_plugin::__private::invoke(
                    #forge_name,
                    #arity_usize,
                    args,
                    argc,
                    out,
                    |__forge_args: ::std::vec::Vec<::forge_plugin::Value>| {
                        let mut __forge_args = __forge_args.into_iter();
                        #(#conversions)*
                        ::forge_plugin::IntoForgeResult::into_forge_result(#ident(#(#call_args),*))
                    },
                )
            }

            impl ::forge_plugin::ForgeFunctionDef for #marker {
                const DEF: ::forge_plugin::abi::ForgeFunction = ::forge_plugin::abi::ForgeFunction {
                    name: #c_name.as_ptr() as *const ::std::os::raw::c_char,
                    arity: #arity,
                    call: ::std::option::Option::Some(__forge_call),
                };
            }
        };
    })
}

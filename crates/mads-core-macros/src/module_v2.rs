//! Expansion for statically registered application modules version 2.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{
    Error, Fields, Ident, ItemStruct, Path, Token, bracketed,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
    spanned::Spanned,
};

use crate::path::core_path;

const SUPPORTED_FORM: &str =
    "`#[mads::module]` supports only unit structs with optional `imports = [Module]` or `global`";

#[derive(Default)]
struct ModuleArguments {
    global: bool,
    imports: Punctuated<Path, Token![,]>,
}

impl Parse for ModuleArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self::default());
        }

        let mut arguments = Self::default();
        let mut global_seen = false;
        let mut imports_seen = false;

        while !input.is_empty() {
            let argument: Ident = input.parse()?;
            match argument.to_string().as_str() {
                "global" => {
                    if global_seen {
                        return Err(Error::new(argument.span(), "duplicate `global` argument"));
                    }
                    arguments.global = true;
                    global_seen = true;
                }
                "imports" => {
                    if imports_seen {
                        return Err(Error::new(argument.span(), "duplicate `imports` argument"));
                    }
                    input.parse::<Token![=]>()?;
                    let content;
                    bracketed!(content in input);
                    arguments.imports = Punctuated::parse_terminated(&content)?;
                    imports_seen = true;
                }
                _ => return Err(Error::new(argument.span(), SUPPORTED_FORM)),
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(arguments)
    }
}

/// Expands a supported module declaration into the declaration and its metadata.
pub(crate) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let arguments: ModuleArguments = syn::parse2(arguments)?;
    let item: ItemStruct = syn::parse2(item)?;
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(item.generics.span(), SUPPORTED_FORM));
    }
    if !matches!(item.fields, Fields::Unit) {
        return Err(Error::new(item.fields.span(), SUPPORTED_FORM));
    }

    let core = core_path()?;
    let ident = &item.ident;
    let module_assertions = arguments.imports.iter().map(|module| {
        quote_spanned! {module.span()=>
            let _ = __mads_assert_module_import::<#module>;
        }
    });
    let import_descriptors = arguments.imports.iter().map(|module| {
        quote! {
            #core::ModuleImportDescriptor::new(
                stringify!(#module),
                || ::core::any::TypeId::of::<#module>(),
            )
        }
    });
    let global = arguments.global.then(|| quote! { .with_global() });

    Ok(quote! {
        #item

        impl #core::Module for #ident {}

        const _: () = {
            fn __mads_assert_module_import<T: #core::Module>() {}
            #(#module_assertions)*
        };

        #core::__private::inventory::submit! {
            #core::ModuleDescriptor::new(
                concat!(module_path!(), "::", stringify!(#ident)),
                || ::core::any::TypeId::of::<#ident>(),
                #core::SourceLocation::new(file!(), line!(), column!()),
            )
            .with_namespace(module_path!())
            .with_imports(&[#(#import_descriptors,)*])
            #global
        }
    })
}
#[cfg(test)]
include!("../tests/support/module_v2.rs");

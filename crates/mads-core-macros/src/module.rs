//! Expansion for statically registered application modules.

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
    "`#[mads::module]` supports only unit structs with optional `imports = [Module]`";

#[derive(Default)]
struct ModuleArguments {
    imports: Punctuated<Path, Token![,]>,
}

impl Parse for ModuleArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self::default());
        }

        let argument: Ident = input.parse()?;
        if argument != "imports" {
            return Err(Error::new(argument.span(), SUPPORTED_FORM));
        }

        input.parse::<Token![=]>()?;
        let content;
        bracketed!(content in input);
        let imports = Punctuated::parse_terminated(&content)?;

        if !input.is_empty() {
            return Err(Error::new(input.span(), SUPPORTED_FORM));
        }

        Ok(Self { imports })
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
        }
    })
}
#[cfg(test)]
include!("../tests/support/module.rs");

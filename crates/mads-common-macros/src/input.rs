//! Validation-only derive with deterministic request-representation paths.

mod attributes;
mod checks;
mod serde_path;

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use serde_path::Naming;
use syn::{Data, DeriveInput, Error, Fields, Generics, Path, Type, parse_quote, spanned::Spanned};

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let mut common = crate::path::common_path()?;
    if common.is_ident("crate") {
        common = parse_quote!(::mads_common);
    }
    let naming = Naming::parse(&input.attrs)?;
    let whole = attributes::parse(&input.attrs, true)?;
    let mut generics = input.generics.clone();
    let body = match &input.data {
        Data::Struct(data) => fields(&data.fields, &naming, &[], &common, &mut generics, false)?.1,
        Data::Enum(data) => {
            let mut arms = Vec::new();
            for variant in &data.variants {
                if variant
                    .attrs
                    .iter()
                    .any(|attr| attr.path().is_ident("validate"))
                {
                    return Err(Error::new_spanned(
                        variant,
                        "put validators on enum fields or the complete enum",
                    ));
                }
                let mut variant_naming = Naming::parse(&variant.attrs)?;
                let name = variant_naming.rename.clone().unwrap_or(serde_path::rename(
                    &variant.ident.to_string(),
                    naming.rename_all.as_deref(),
                    true,
                )?);
                let prefix = if naming.untagged || variant_naming.untagged {
                    vec![]
                } else if let Some(content) = &naming.content {
                    vec![quote!(#common::ValidationPathSegment::Field(#content.into()))]
                } else if naming.tag.is_some() {
                    vec![]
                } else {
                    vec![quote!(#common::ValidationPathSegment::Field(#name.into()))]
                };
                if variant_naming.rename_all.is_none() {
                    variant_naming.rename_all = naming.rename_all_fields.clone();
                }
                let (pattern, body) = fields(
                    &variant.fields,
                    &variant_naming,
                    &prefix,
                    &common,
                    &mut generics,
                    true,
                )?;
                let ident = &variant.ident;
                arms.push(quote!(Self::#ident #pattern => { #body }));
            }
            quote!(match self { #(#arms),* })
        }
        Data::Union(_) => {
            return Err(Error::new_spanned(
                &input.ident,
                "Input cannot be derived for a union",
            ));
        }
    };
    let callbacks = whole.iter().map(|validator| {
        let callback = validator.callback.as_ref().expect("whole custom validator");
        // A direct call preserves argument coercions such as &Self to &dyn Trait.
        quote_spanned! {callback.span()=>
            {
                let __mads_whole_result: #common::ValidationResult = #callback(self);
                #common::__private::input_validation::merge(
                    &mut __mads_errors,
                    __mads_whole_result,
                );
            }
        }
    });
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #common::Input for #ident #ty_generics #where_clause {
            fn validate(&self) -> #common::ValidationResult {
                let mut __mads_errors: Option<#common::ValidationErrors> = None;
                #body
                #(#callbacks)*
                #common::__private::input_validation::finish(__mads_errors)
            }
        }
    })
}

fn fields(
    fields: &Fields,
    naming: &Naming,
    prefix: &[TokenStream],
    common: &Path,
    generics: &mut Generics,
    variant: bool,
) -> syn::Result<(TokenStream, TokenStream)> {
    let mut patterns = Vec::new();
    let mut bodies = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let binding = format_ident!("__mads_field_{index}");
        let field_naming = Naming::parse(&field.attrs)?;
        let mut path = prefix.to_vec();
        let access = if let Some(ident) = &field.ident {
            patterns.push(quote!(#ident: #binding));
            if !field_naming.flatten {
                let name = field_naming.rename.unwrap_or(serde_path::rename(
                    &ident.to_string(),
                    naming.rename_all.as_deref(),
                    false,
                )?);
                path.push(quote!(#common::ValidationPathSegment::Field(#name.into())));
            }
            quote!(&self.#ident)
        } else {
            patterns.push(quote!(#binding));
            if !field_naming.flatten {
                path.push(quote!(#common::ValidationPathSegment::Index(#index)));
            }
            let index = syn::Index::from(index);
            quote!(&self.#index)
        };
        let validators = attributes::parse(&field.attrs, false)?;
        let value = if variant { quote!(#binding) } else { access };
        let body = checks::expand(&field.ty, &validators, common, generics)?;
        bodies.push(quote! {
            {
                let __mads_value = #value;
                let __mads_path: &[#common::ValidationPathSegment] = &[#(#path),*];
                #body
            }
        });
    }
    let pattern = match fields {
        Fields::Named(_) => quote!({#(#patterns),*}),
        Fields::Unnamed(_) => quote!((#(#patterns),*)),
        Fields::Unit => quote!(),
    };
    Ok((pattern, quote!(#(#bodies)*)))
}

fn type_name(ty: &Type) -> String {
    match ungroup(ty) {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn arguments(ty: &Type) -> Vec<&Type> {
    if let Type::Path(path) = ungroup(ty) {
        if let Some(segment) = path.path.segments.last() {
            if let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments {
                return arguments
                    .args
                    .iter()
                    .filter_map(|arg| {
                        if let syn::GenericArgument::Type(ty) = arg {
                            Some(ty)
                        } else {
                            None
                        }
                    })
                    .collect();
            }
        }
    }
    Vec::new()
}

fn ungroup(ty: &Type) -> &Type {
    match ty {
        Type::Group(group) => ungroup(&group.elem),
        Type::Paren(paren) => ungroup(&paren.elem),
        _ => ty,
    }
}

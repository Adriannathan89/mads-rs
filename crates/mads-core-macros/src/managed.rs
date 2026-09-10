//! Shared expansion for service and repository managed providers.

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Error, ExprPath, Fields, Ident, ItemStruct, Type, TypePath, spanned::Spanned,
};

use crate::path::core_path;

/// Selects the provider role emitted by the shared managed expansion.
pub(crate) enum ManagedKind {
    /// An application service.
    Service,
    /// A persistence repository.
    Repository,
}

impl ManagedKind {
    fn attribute_name(&self) -> &'static str {
        match self {
            Self::Service => "service",
            Self::Repository => "repository",
        }
    }

    fn provider_kind(&self, core: &syn::Path) -> TokenStream {
        match self {
            Self::Service => quote!(#core::ProviderKind::Service),
            Self::Repository => quote!(#core::ProviderKind::Repository),
        }
    }

    fn supported_form(&self) -> String {
        let attribute = self.attribute_name();
        format!(
            "`#[mads::{attribute}]` supports only non-generic named-field or unit structs, such as `#[mads::{attribute}] struct Managed {{ dependency: Dependency }}` or `#[mads::{attribute}] struct Managed;`"
        )
    }
}

/// Expands a supported service or repository into a handle and provider metadata.
pub(crate) fn expand(
    kind: ManagedKind,
    arguments: TokenStream,
    item: TokenStream,
) -> syn::Result<TokenStream> {
    if !arguments.is_empty() {
        return Err(Error::new(arguments.span(), kind.supported_form()));
    }

    let item: ItemStruct = syn::parse2(item)?;
    expand_managed(kind, item)
}

fn expand_managed(kind: ManagedKind, item: ItemStruct) -> syn::Result<TokenStream> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(item.generics.span(), kind.supported_form()));
    }

    if let Fields::Unnamed(fields) = &item.fields {
        return Err(Error::new(fields.span(), kind.supported_form()));
    }

    if let Some(attribute) = item.attrs.iter().find(is_repr) {
        return Err(Error::new(
            attribute.span(),
            format!(
                "representation attributes are not supported on `#[mads::{}]` structs; remove this `#[repr(...)]` attribute",
                kind.attribute_name()
            ),
        ));
    }

    if let Some(attribute) = item.attrs.iter().find(|attribute| !is_doc(attribute)) {
        return Err(Error::new(
            attribute.span(),
            format!(
                "`#[mads::{}]` managed providers support documentation attributes only in v0.2; remove this struct attribute",
                kind.attribute_name()
            ),
        ));
    }

    if let Fields::Named(fields) = &item.fields {
        for field in &fields.named {
            if let Some(attribute) = field.attrs.iter().find(|attribute| !is_doc(attribute)) {
                return Err(Error::new(
                    attribute.span(),
                    format!(
                        "`#[mads::{}]` managed-provider fields support documentation attributes only in v0.2; remove this field attribute",
                        kind.attribute_name()
                    ),
                ));
            }
        }
    }

    let core = core_path()?;
    expand_managed_with_core(kind, item, core)
}

fn expand_managed_with_core(
    kind: ManagedKind,
    item: ItemStruct,
    core: syn::Path,
) -> syn::Result<TokenStream> {
    let provider_kind = kind.provider_kind(&core);
    let provider_visibility = provider_visibility(&item.vis, &core);
    let ItemStruct {
        attrs,
        vis,
        ident,
        fields,
        ..
    } = item;
    let inner_ident = format_ident!("__Mads{}Inner", ident);
    let constructor_ident = format_ident!("__mads_construct_{}", ident);
    let type_id_ident = format_ident!("__mads_type_id_{}", ident);
    let runtime_type_name_ident = format_ident!("__mads_runtime_type_name_{}", ident);
    let is_unit = matches!(fields, Fields::Unit);

    let (inner_fields, resolve_fields, dependencies) = match fields {
        Fields::Named(fields) => {
            let normalized_fields: Vec<_> = fields
                .named
                .iter()
                .cloned()
                .map(|mut field| {
                    let dependency_span = field.ty.span();
                    normalize_self_type(&mut field.ty, &ident);
                    (field, dependency_span)
                })
                .collect();
            let declarations = normalized_fields.iter().map(|(field, _)| {
                let attrs = &field.attrs;
                let vis = &field.vis;
                let ident = &field.ident;
                let ty = &field.ty;
                quote!(#(#attrs)* #vis #ident: #ty)
            });
            let resolutions = normalized_fields.iter().map(|(field, dependency_span)| {
                let ident = field.ident.as_ref().expect("named fields have identifiers");
                let ty = &field.ty;
                quote_spanned! {*dependency_span=>
                    #ident: __mads_assert_managed_dependency::<#ty>(context)?
                }
            });
            let descriptors = normalized_fields.iter().map(|(field, _)| {
                let ty = &field.ty;
                quote! {
                    #core::DependencyDescriptor::new(
                        stringify!(#ty),
                        || ::core::any::TypeId::of::<#ty>(),
                    )
                }
            });
            (
                quote!({ #(#declarations,)* }),
                quote!({ #(#resolutions,)* }),
                quote!(&[#(#descriptors,)*]),
            )
        }
        Fields::Unit => (quote!(;), quote!(), quote!(&[])),
        Fields::Unnamed(_) => unreachable!("tuple fields were rejected above"),
    };

    let inner_value = if is_unit {
        quote!(#inner_ident)
    } else {
        quote!(#inner_ident #resolve_fields)
    };

    Ok(quote! {
        #[doc(hidden)]
        #vis struct #inner_ident #inner_fields

        #(#attrs)*
        #vis struct #ident(::std::sync::Arc<#inner_ident>);

        impl ::core::clone::Clone for #ident {
            fn clone(&self) -> Self {
                Self(::std::sync::Arc::clone(&self.0))
            }
        }

        impl ::core::ops::Deref for #ident {
            type Target = #inner_ident;

            fn deref(&self) -> &Self::Target {
                self.0.as_ref()
            }
        }

        const _: () = {
            fn __mads_assert_managed_dependency<'a, T>(
                context: &'a #core::ConstructionContext<'a>,
            ) -> #core::Result<T>
            where
                T: ::core::clone::Clone
                    + ::core::marker::Send
                    + ::core::marker::Sync
                    + 'static,
            {
                Ok(::core::clone::Clone::clone(context.resolve::<T>()?.as_ref()))
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #constructor_ident<'a>(
                context: &'a #core::ConstructionContext<'a>,
            ) -> #core::ProviderFuture<'a> {
                ::std::boxed::Box::pin(async move {
                    let value = #ident(::std::sync::Arc::new(#inner_value));
                    let erased: #core::ErasedProvider = ::std::sync::Arc::new(value);
                    Ok(erased)
                })
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #type_id_ident() -> ::core::any::TypeId {
                ::core::any::TypeId::of::<#ident>()
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #runtime_type_name_ident() -> &'static str {
                ::core::any::type_name::<#ident>()
            }

            #core::__private::inventory::submit! {
                #core::ProviderDescriptor::new(
                    #provider_kind,
                    concat!(module_path!(), "::", stringify!(#ident)),
                    #type_id_ident,
                    #dependencies,
                    #provider_visibility,
                    #core::SourceLocation::new(file!(), line!(), column!()),
                    #constructor_ident,
                )
                .with_runtime_type_name(#runtime_type_name_ident)
                .with_namespace(module_path!())
            }
        };
    })
}

fn provider_visibility(visibility: &syn::Visibility, core: &syn::Path) -> TokenStream {
    if matches!(visibility, syn::Visibility::Public(_)) {
        quote!(#core::ProviderVisibility::Public)
    } else {
        quote!(#core::ProviderVisibility::Private)
    }
}

fn is_repr(attribute: &&Attribute) -> bool {
    attribute.path().is_ident("repr")
}

fn is_doc(attribute: &&Attribute) -> bool {
    attribute.path().is_ident("doc")
}

fn normalize_self_type(ty: &mut Type, handle: &Ident) {
    SelfTypeNormalizer { handle }.visit_type_mut(ty);
}

struct SelfTypeNormalizer<'a> {
    handle: &'a Ident,
}

impl VisitMut for SelfTypeNormalizer<'_> {
    fn visit_expr_path_mut(&mut self, expression_path: &mut ExprPath) {
        if expression_path.qself.is_none() {
            if let Some(segment) = expression_path.path.segments.first_mut() {
                if segment.ident == "Self" {
                    segment.ident = self.handle.clone();
                }
            }
        }

        visit_mut::visit_expr_path_mut(self, expression_path);
    }

    fn visit_type_path_mut(&mut self, type_path: &mut TypePath) {
        if type_path.qself.is_none() {
            if let Some(segment) = type_path.path.segments.first_mut() {
                if segment.ident == "Self" {
                    segment.ident = self.handle.clone();
                }
            }
        }

        visit_mut::visit_type_path_mut(self, type_path);
    }
}
#[cfg(test)]
include!("../tests/support/managed.rs");

//! Expansion for synchronous and asynchronous provider functions.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::visit::{self, Visit};
use syn::{
    Error, Expr, FnArg, GenericArgument, ItemFn, PathArguments, ReturnType, Type, spanned::Spanned,
};

use crate::path::core_path;

/// Expands a provider function into the original function and registered constructor metadata.
pub(crate) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if !arguments.is_empty() {
        return Err(Error::new(
            arguments.span(),
            "`#[mads::provider]` does not accept arguments",
        ));
    }

    let item: ItemFn = syn::parse2(item)?;
    validate_signature(&item)?;
    expand_provider(item)
}

fn validate_signature(item: &ItemFn) -> syn::Result<()> {
    if let Some(receiver) = item.sig.inputs.iter().find_map(|argument| match argument {
        FnArg::Receiver(receiver) => Some(receiver),
        FnArg::Typed(_) => None,
    }) {
        return Err(Error::new(
            receiver.span(),
            "`#[mads::provider]` cannot be applied to methods; declare a free function without a `self` receiver",
        ));
    }

    if let Some(parameter) = item.sig.generics.params.first() {
        return Err(Error::new(
            parameter.span(),
            "`#[mads::provider]` does not support lifetime, type, or const generics",
        ));
    }

    if let Some(variadic) = &item.sig.variadic {
        return Err(Error::new(
            variadic.span(),
            "`#[mads::provider]` does not support variadic functions",
        ));
    }

    if let Some(unsafety) = &item.sig.unsafety {
        return Err(Error::new(
            unsafety.span(),
            "`#[mads::provider]` does not support unsafe functions",
        ));
    }

    match &item.sig.output {
        ReturnType::Default => Err(Error::new(
            item.sig.ident.span(),
            "`#[mads::provider]` requires an explicit concrete return type",
        )),
        ReturnType::Type(_, output) => {
            non_concrete_output_span(output).map_or(Ok(()), |non_concrete| {
                Err(Error::new(
                    non_concrete,
                    "`#[mads::provider]` requires an explicit concrete return type",
                ))
            })
        }
    }
}

fn expand_provider(item: ItemFn) -> syn::Result<TokenStream> {
    let core = core_path()?;
    expand_provider_with_core(item, core)
}

fn expand_provider_with_core(item: ItemFn, core: syn::Path) -> syn::Result<TokenStream> {
    let provider_visibility = provider_visibility(&item.vis, &core);
    let ident = &item.sig.ident;
    let type_id_ident = format_ident!("__mads_type_id_{ident}");
    let runtime_type_name_ident = format_ident!("__mads_runtime_type_name_{ident}");
    let return_type = match &item.sig.output {
        ReturnType::Type(_, return_type) => return_type.as_ref(),
        ReturnType::Default => unreachable!("provider return type was validated"),
    };
    let (output_type, fallible) =
        result_output(return_type).map_or((return_type, false), |output_type| (output_type, true));

    let dependencies: Vec<_> = item
        .sig
        .inputs
        .iter()
        .filter_map(|argument| match argument {
            FnArg::Typed(argument) => Some(argument.ty.as_ref()),
            FnArg::Receiver(_) => None,
        })
        .collect();
    let dependency_idents: Vec<_> = (0..dependencies.len())
        .map(|index| format_ident!("__mads_dependency_{index}"))
        .collect();
    let resolve_dependencies =
        dependencies
            .iter()
            .zip(&dependency_idents)
            .map(|(dependency, dependency_ident)| {
                quote_spanned! {dependency.span()=>
                    let #dependency_ident =
                        __mads_assert_provider_dependency::<#dependency>(context)?;
                }
            });
    let dependency_descriptors = dependencies.iter().map(|dependency| {
        quote! {
            #core::DependencyDescriptor::new(
                stringify!(#dependency),
                || ::core::any::TypeId::of::<#dependency>(),
            )
        }
    });
    let call = quote!(#ident(#(#dependency_idents),*));
    let call = if item.sig.asyncness.is_some() {
        quote!(#call.await)
    } else {
        call
    };
    let construct_value = if fallible {
        quote!(let value: #output_type = #call?;)
    } else {
        quote!(let value: #output_type = #call;)
    };

    Ok(quote! {
        #item

        const _: () = {
            fn __mads_assert_provider_dependency<'a, T>(
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
            fn __mads_construct<'a>(
                context: &'a #core::ConstructionContext<'a>,
            ) -> #core::ProviderFuture<'a> {
                ::std::boxed::Box::pin(async move {
                    #(#resolve_dependencies)*
                    #construct_value
                    let erased: #core::ErasedProvider = ::std::sync::Arc::new(value);
                    Ok(erased)
                })
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #type_id_ident() -> ::core::any::TypeId {
                ::core::any::TypeId::of::<#output_type>()
            }

            #[doc(hidden)]
            #[allow(non_snake_case)]
            fn #runtime_type_name_ident() -> &'static str {
                ::core::any::type_name::<#output_type>()
            }

            #core::__private::inventory::submit! {
                #core::ProviderDescriptor::new(
                    #core::ProviderKind::Provider,
                    stringify!(#output_type),
                    #type_id_ident,
                    &[#(#dependency_descriptors,)*],
                    #provider_visibility,
                    #core::SourceLocation::new(file!(), line!(), column!()),
                    __mads_construct,
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

fn non_concrete_output_span(output: &Type) -> Option<Span> {
    let mut finder = NonConcreteOutputFinder { found: None };
    finder.visit_type(output);
    finder.found
}

struct NonConcreteOutputFinder {
    found: Option<Span>,
}

impl<'ast> Visit<'ast> for NonConcreteOutputFinder {
    fn visit_type(&mut self, node: &'ast Type) {
        if self.found.is_some() {
            return;
        }
        if matches!(node, Type::Infer(_) | Type::ImplTrait(_)) {
            self.found = Some(node.span());
            return;
        }
        visit::visit_type(self, node);
    }

    fn visit_expr(&mut self, node: &'ast Expr) {
        if self.found.is_some() {
            return;
        }
        if matches!(node, Expr::Infer(_)) {
            self.found = Some(node.span());
            return;
        }
        visit::visit_expr(self, node);
    }
}

fn result_output(return_type: &Type) -> Option<&Type> {
    let return_type = ungroup_type(return_type);
    let Type::Path(type_path) = return_type else {
        return None;
    };
    if type_path.qself.is_some() {
        return None;
    }

    let names: Vec<_> = type_path
        .path
        .segments
        .iter()
        .map(|segment| segment.ident.to_string())
        .collect();
    let recognized = match names.as_slice() {
        [result] => result == "Result",
        [core, result] => core == "mads_core" && result == "Result",
        [facade, core, result] => facade == "mads" && core == "core" && result == "Result",
        _ => false,
    };
    if !recognized {
        return None;
    }

    let PathArguments::AngleBracketed(arguments) = &type_path.path.segments.last()?.arguments
    else {
        return None;
    };
    if arguments.args.len() != 1 {
        return None;
    }
    match arguments.args.first()? {
        GenericArgument::Type(output) => Some(output),
        _ => None,
    }
}

fn ungroup_type(mut type_: &Type) -> &Type {
    loop {
        type_ = match type_ {
            Type::Group(group) => &group.elem,
            Type::Paren(parenthesized) => &parenthesized.elem,
            _ => return type_,
        };
    }
}
#[cfg(test)]
include!("../tests/support/provider.rs");

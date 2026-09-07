//! Expansion for compile-time route-contract traits.
//!
//! This module owns the route grammar used by `#[routes]` and the HTTP verb
//! attributes. Validation happens before metadata is emitted so malformed
//! paths cannot reach a runtime adapter. The generated trait retains the
//! developer's method documentation and gains hidden contract/metadata
//! constants plus a typed Axum registrar used by `#[controller]`.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{
    Attribute, Error, FnArg, ItemTrait, LitStr, Meta, ReturnType, Token, TraitItem, TraitItemFn,
    Type, parse_quote, spanned::Spanned,
};

use crate::guard::{self, GuardSpec, GuardTarget};

const CONTRACT_MARKER: &str = "__MADS_ROUTE_CONTRACT";
const VERBS: &[&str] = &["get", "post", "put", "patch", "delete"];

struct RoutesArguments {
    prefix: Option<LitStr>,
}

impl Parse for RoutesArguments {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        if input.is_empty() {
            return Ok(Self { prefix: None });
        }

        let name: syn::Ident = input.parse()?;
        if name != "prefix" {
            return Err(Error::new(name.span(), "expected `prefix = \"/...\"`"));
        }
        input.parse::<Token![=]>()?;
        let prefix: LitStr = input.parse()?;
        if !input.is_empty() {
            return Err(input.error("`#[routes]` accepts only one `prefix` argument"));
        }

        Ok(Self {
            prefix: Some(prefix),
        })
    }
}

/// Expands a route trait after validating its endpoint contract.
pub(crate) fn expand(arguments: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let common = crate::path::common_path()?;
    expand_with_common(arguments, item, &common)
}

fn expand_with_common(
    arguments: TokenStream,
    item: TokenStream,
    common: &syn::Path,
) -> syn::Result<TokenStream> {
    let arguments = syn::parse2::<RoutesArguments>(arguments)?;
    if let Some(prefix) = &arguments.prefix {
        validate_path(prefix, "route prefix", true)?;
    }
    let prefix = arguments
        .prefix
        .unwrap_or_else(|| LitStr::new("", proc_macro2::Span::call_site()));

    let mut item: ItemTrait = syn::parse2(item)?;
    validate_trait_shape(&item)?;

    if !cfg!(feature = "passport") {
        if let Some(attribute) = first_guard_attribute(&item) {
            return Err(Error::new(
                attribute.span(),
                "guards require the `jwt` feature",
            ));
        }
    }
    let trait_guard = guard::take_guard(&mut item.attrs, GuardTarget::Trait)?;
    if let Some(trait_guard) = &trait_guard {
        guard::validate_trait_guard(trait_guard, item.ident.span())?;
    }

    let mut routes = BTreeSet::new();
    let mut descriptors = Vec::new();
    let mut route_count = 0usize;
    for trait_item in &mut item.items {
        let TraitItem::Fn(method) = trait_item else {
            return Err(Error::new(
                trait_item.span(),
                "`#[routes]` traits may contain route methods only",
            ));
        };
        let method_guard = guard::take_guard(&mut method.attrs, GuardTarget::Method)?;
        descriptors.push(validate_method_with_guard(
            method,
            &mut routes,
            &prefix,
            common,
            trait_guard.as_ref(),
            method_guard,
        )?);
        route_count += 1;
    }

    if route_count == 0 {
        return Err(Error::new(
            item.ident.span(),
            "`#[routes]` requires at least one route method",
        ));
    }

    let marker_ident = syn::Ident::new(CONTRACT_MARKER, item.ident.span());
    if item.items.iter().any(|trait_item| match trait_item {
        TraitItem::Const(value) => value.ident == marker_ident,
        _ => false,
    }) {
        return Err(Error::new(
            item.ident.span(),
            format!("`{CONTRACT_MARKER}` is reserved by `#[routes]`"),
        ));
    }

    let marker: TraitItem = parse_quote! {
        #[doc(hidden)]
        const __MADS_ROUTE_CONTRACT: () = ();
    };
    item.items.insert(0, marker);

    let trait_ident = &item.ident;
    let guard_statics = descriptors
        .iter_mut()
        .filter_map(|descriptor| {
            let guard = descriptor.guard.take()?;
            let (identifier, tokens) = guard.static_tokens(
                common,
                trait_ident,
                &descriptor.handler_ident,
                &descriptor.conditional_attributes,
            );
            descriptor.guard_ident = Some(identifier);
            Some(tokens)
        })
        .collect::<Vec<_>>();

    let descriptor_tokens = descriptors.iter().map(|descriptor| {
        let conditional_attributes = &descriptor.conditional_attributes;
        let method = descriptor.method.tokens(common);
        let path = &descriptor.path;
        let full_path = &descriptor.full_path;
        let handler = &descriptor.handler;
        let guard = descriptor
            .guard_ident
            .as_ref()
            .map(|guard| quote!(.with_guard(&#guard)));
        quote! {
            #(#conditional_attributes)*
            #common::RouteDescriptor::new(
                #method,
                #prefix,
                #path,
                #full_path,
                #handler,
                #common::core::SourceLocation::new(file!(), line!(), column!()),
            )
            #guard
            .with_namespace(module_path!())
        }
    });
    let metadata: TraitItem = parse_quote! {
        #[doc(hidden)]
        const __MADS_ROUTE_METADATA: &'static [#common::RouteDescriptor] = &[
            #(#descriptor_tokens,)*
        ];
    };
    item.items.insert(1, metadata);

    let registrations = descriptors
        .iter()
        .map(|descriptor| descriptor.registration_tokens(common, trait_ident));
    let registrar: TraitItem = parse_quote! {
        #[doc(hidden)]
        fn __mads_register(
            mut __mads_router: #common::__private::Router,
            __mads_controller: Self,
            __mads_runtime: &#common::__private::RouterBuildContext<'_>,
            __mads_routes: &mut #common::__private::ValidatedRouteIter<'_>,
        ) -> #common::core::Result<#common::__private::Router>
        where
            Self: ::core::clone::Clone
                + ::core::marker::Send
                + ::core::marker::Sync
                + 'static,
        {
            #(#registrations)*
            Ok(__mads_router)
        }
    };
    item.items.insert(2, registrar);

    Ok(quote!(
        #(#guard_statics)*
        #item
    ))
}

fn first_guard_attribute(item: &ItemTrait) -> Option<&Attribute> {
    item.attrs
        .iter()
        .find(|attribute| guard::is_guard_attribute(attribute))
        .or_else(|| {
            item.items.iter().find_map(|item| match item {
                TraitItem::Fn(method) => method
                    .attrs
                    .iter()
                    .find(|attribute| guard::is_guard_attribute(attribute)),
                _ => None,
            })
        })
}

fn validate_trait_shape(item: &ItemTrait) -> syn::Result<()> {
    if !item.generics.params.is_empty() || item.generics.where_clause.is_some() {
        return Err(Error::new(
            item.generics.span(),
            "`#[routes]` does not support generic traits",
        ));
    }
    if let Some(unsafety) = &item.unsafety {
        return Err(Error::new(
            unsafety.span(),
            "`#[routes]` does not support unsafe traits",
        ));
    }
    if let Some(auto_token) = &item.auto_token {
        return Err(Error::new(
            auto_token.span(),
            "`#[routes]` does not support auto traits",
        ));
    }
    Ok(())
}

#[cfg(test)]
fn validate_method(
    method: &mut TraitItemFn,
    routes: &mut BTreeSet<(String, String)>,
    prefix: &LitStr,
) -> syn::Result<RouteMetadata> {
    validate_method_with_guard(
        method,
        routes,
        prefix,
        &parse_quote!(mads_common),
        None,
        None,
    )
}

fn validate_method_with_guard(
    method: &mut TraitItemFn,
    routes: &mut BTreeSet<(String, String)>,
    prefix: &LitStr,
    common: &syn::Path,
    trait_guard: Option<&GuardSpec>,
    method_guard: Option<GuardSpec>,
) -> syn::Result<RouteMetadata> {
    if method.default.is_some() {
        return Err(Error::new(
            method.span(),
            "route contract methods must not provide default implementations",
        ));
    }
    if method.sig.asyncness.is_none() {
        return Err(Error::new(
            method.sig.fn_token.span(),
            "route contract methods must be async",
        ));
    }
    if method.sig.constness.is_some()
        || method.sig.unsafety.is_some()
        || method.sig.abi.is_some()
        || method.sig.variadic.is_some()
        || !method.sig.generics.params.is_empty()
        || method.sig.generics.where_clause.is_some()
    {
        return Err(Error::new(
            method.sig.span(),
            "route contract methods cannot be const, unsafe, extern, variadic, or generic",
        ));
    }

    let Some(FnArg::Receiver(receiver)) = method.sig.inputs.first() else {
        return Err(Error::new(
            method.sig.inputs.span(),
            "route contract methods require `&self` as the first parameter",
        ));
    };
    if receiver.reference.is_none()
        || receiver.mutability.is_some()
        || receiver.colon_token.is_some()
    {
        return Err(Error::new(
            receiver.span(),
            "route contract methods require an immutable `&self` receiver",
        ));
    }
    validate_body_extractor_order(&method.sig.inputs, common)?;

    let mut route_attributes = Vec::new();
    let mut conditional_attributes = Vec::new();
    for (index, attribute) in method.attrs.iter().enumerate() {
        if attribute.path().is_ident("cfg_attr") && cfg_attr_contains_route_verb(attribute)? {
            return Err(Error::new(
                attribute.span(),
                "route verb attributes inside `cfg_attr` are unsupported; use a direct route verb and gate the method with `#[cfg(...)]`",
            ));
        }
        if let Some(verb) = route_verb(attribute) {
            route_attributes.push((index, verb));
        }
        if is_conditional_attribute(attribute) {
            conditional_attributes.push(attribute.clone());
        }
    }
    if route_attributes.len() != 1 {
        if method_guard.is_some() {
            return Err(Error::new(
                method.sig.ident.span(),
                "`#[guard]` must appear below one route verb on a method inside `#[routes]`",
            ));
        }
        return Err(Error::new(
            method.sig.ident.span(),
            "each route contract method requires exactly one of `#[get]`, `#[post]`, `#[put]`, `#[patch]`, or `#[delete]`",
        ));
    }

    let (attribute_index, verb) = &route_attributes[0];
    if let Some(method_guard) = method_guard.as_ref() {
        let route_attribute_index = if method_guard
            .attribute_index()
            .is_some_and(|guard_index| *attribute_index >= guard_index)
        {
            *attribute_index + 1
        } else {
            *attribute_index
        };
        if method_guard.attribute_index() != Some(route_attribute_index + 1) {
            return Err(Error::new(
                method_guard.attribute_span(),
                "`#[guard]` must appear directly below the route verb on a method inside `#[routes]`",
            ));
        }
    }
    let attribute = &method.attrs[*attribute_index];
    let path = parse_route_path(attribute)?;
    validate_path(&path, "route path", false)?;

    let full_path = join_paths(prefix, &path)?;
    let guard = guard::merge(trait_guard, method_guard.as_ref(), method.sig.ident.span())?;

    let identity = ((*verb).to_owned(), full_path.value());
    if !routes.insert(identity) {
        return Err(Error::new(
            attribute.span(),
            "duplicate HTTP verb and path in this route contract",
        ));
    }

    method.attrs.remove(*attribute_index);
    let argument_types = method
        .sig
        .inputs
        .iter()
        .skip(1)
        .map(|argument| match argument {
            FnArg::Typed(argument) => (*argument.ty).clone(),
            FnArg::Receiver(_) => unreachable!("only the first route argument can be a receiver"),
        })
        .collect();
    make_future_send(method);
    Ok(RouteMetadata {
        method: HttpVerb::from_name(verb),
        path,
        full_path,
        handler: LitStr::new(&method.sig.ident.to_string(), method.sig.ident.span()),
        handler_ident: method.sig.ident.clone(),
        argument_types,
        conditional_attributes,
        guard,
        guard_ident: None,
    })
}

/// Identifies body-consuming extractors whose crate path is known to MADS.
///
/// This intentionally inspects syntax only. A bare `Json` or an application
/// extractor with a different path may resolve to any type, so its body
/// behavior remains the native Axum/rustc contract.
fn validate_body_extractor_order(
    inputs: &Punctuated<FnArg, Token![,]>,
    common: &syn::Path,
) -> syn::Result<()> {
    let arguments = inputs.iter().skip(1).collect::<Vec<_>>();
    for (index, argument) in arguments.iter().enumerate() {
        let FnArg::Typed(argument) = argument else {
            continue;
        };
        if known_body_consumer(&argument.ty, common) && index + 1 != arguments.len() {
            return Err(Error::new(
                argument.ty.span(),
                "known body extractors (`Json`, `ValidatedJson`, and `Request`) must be the final route parameter",
            ));
        }
    }
    Ok(())
}

fn known_body_consumer(ty: &Type, common: &syn::Path) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }

    let path = &type_path.path;
    if is_mads_type(path, common, "Json") || is_axum_type(path, common, "Json") {
        return true;
    }
    if is_mads_type(path, common, "ValidatedJson") {
        return true;
    }
    if is_mads_type(path, common, "Request") || is_axum_type(path, common, "Request") {
        return true;
    }
    false
}

fn is_mads_type(path: &syn::Path, common: &syn::Path, name: &str) -> bool {
    path_is(path, &["mads", name])
        || path_is(path, &["mads", "common", name])
        || path_is(path, &["mads_common", name])
        || path_with_suffix_is(path, common, &[name])
        || facade_path_with_suffix_is(path, common, &[name])
}

fn is_axum_type(path: &syn::Path, common: &syn::Path, name: &str) -> bool {
    path_is(path, &["axum", name])
        || path_is(path, &["axum", "extract", name])
        || path_is(path, &["mads", "axum", name])
        || path_is(path, &["mads", "axum", "extract", name])
        || path_is(path, &["mads", "common", "axum", name])
        || path_is(path, &["mads", "common", "axum", "extract", name])
        || path_is(path, &["mads_common", "axum", name])
        || path_is(path, &["mads_common", "axum", "extract", name])
        || path_with_suffix_is(path, common, &["axum", name])
        || path_with_suffix_is(path, common, &["axum", "extract", name])
        || facade_path_with_suffix_is(path, common, &["axum", name])
        || facade_path_with_suffix_is(path, common, &["axum", "extract", name])
}

fn facade_path_with_suffix_is(path: &syn::Path, common: &syn::Path, suffix: &[&str]) -> bool {
    let Some(last) = common.segments.last() else {
        return false;
    };
    if last.ident != "common" {
        return false;
    }

    let prefix_len = common.segments.len() - 1;
    let path_len = path.segments.len();
    path_len == prefix_len + suffix.len()
        && path
            .segments
            .iter()
            .take(prefix_len)
            .zip(common.segments.iter().take(prefix_len))
            .all(|(actual, expected)| actual.ident == expected.ident)
        && path
            .segments
            .iter()
            .skip(prefix_len)
            .zip(suffix)
            .all(|(segment, expected)| segment.ident == *expected)
}

fn path_with_suffix_is(path: &syn::Path, prefix: &syn::Path, suffix: &[&str]) -> bool {
    let prefix_len = prefix.segments.len();
    let path_len = path.segments.len();
    path_len == prefix_len + suffix.len()
        && path
            .segments
            .iter()
            .take(prefix_len)
            .zip(prefix.segments.iter())
            .all(|(actual, expected)| actual.ident == expected.ident)
        && path
            .segments
            .iter()
            .skip(prefix_len)
            .zip(suffix)
            .all(|(segment, expected)| segment.ident == *expected)
}

fn path_is(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments.len() == expected.len()
        && path
            .segments
            .iter()
            .zip(expected)
            .all(|(segment, expected)| segment.ident == *expected)
}

struct RouteMetadata {
    method: HttpVerb,
    path: LitStr,
    full_path: LitStr,
    handler: LitStr,
    handler_ident: syn::Ident,
    argument_types: Vec<Type>,
    conditional_attributes: Vec<Attribute>,
    guard: Option<guard::EffectiveGuard>,
    guard_ident: Option<syn::Ident>,
}

impl RouteMetadata {
    fn registration_tokens(&self, common: &syn::Path, trait_ident: &syn::Ident) -> TokenStream {
        let method = self.method.tokens(common);
        let routing = self.method.routing_tokens(common);
        let handler = &self.handler;
        let handler_ident = &self.handler_ident;
        let argument_types = &self.argument_types;
        let conditional_attributes = &self.conditional_attributes;
        let guard_layer = self.guard_ident.as_ref().map(|guard| {
            quote! {
                .route_layer(#common::__private::PassportGuardLayer::new(
                    __mads_runtime.passport_guard_state(
                        &#guard,
                        __mads_routes.passport_context_module(),
                    )?,
                ))
            }
        });
        let arguments = argument_types
            .iter()
            .enumerate()
            .map(|(index, _)| format_ident!("__mads_argument_{index}"))
            .collect::<Vec<_>>();

        quote! {
            #(#conditional_attributes)*
            {
                if let Some(__mads_path) = __mads_routes.next(#method, #handler)? {
                    let __mads_handler_controller = __mads_controller.clone();
                    __mads_router = __mads_router.route(
                        __mads_path,
                        #routing(move |#(#arguments: #argument_types),*| {
                            let __mads_controller = __mads_handler_controller.clone();
                            async move {
                                <Self as #trait_ident>::#handler_ident(
                                    &__mads_controller,
                                    #(#arguments,)*
                                ).await
                            }
                        })
                        #guard_layer,
                    );
                }
            }
        }
    }
}

fn is_conditional_attribute(attribute: &Attribute) -> bool {
    attribute.path().is_ident("cfg") || attribute.path().is_ident("cfg_attr")
}

#[derive(Clone, Copy)]
enum HttpVerb {
    Get,
    Post,
    Put,
    Patch,
    Delete,
}

impl HttpVerb {
    fn from_name(name: &str) -> Self {
        match name {
            "get" => Self::Get,
            "post" => Self::Post,
            "put" => Self::Put,
            "patch" => Self::Patch,
            "delete" => Self::Delete,
            _ => unreachable!("validated route verbs are exhaustive"),
        }
    }

    fn tokens(self, common: &syn::Path) -> proc_macro2::TokenStream {
        match self {
            Self::Get => quote!(#common::HttpMethod::Get),
            Self::Post => quote!(#common::HttpMethod::Post),
            Self::Put => quote!(#common::HttpMethod::Put),
            Self::Patch => quote!(#common::HttpMethod::Patch),
            Self::Delete => quote!(#common::HttpMethod::Delete),
        }
    }

    fn routing_tokens(self, common: &syn::Path) -> proc_macro2::TokenStream {
        match self {
            Self::Get => quote!(#common::__private::get),
            Self::Post => quote!(#common::__private::post),
            Self::Put => quote!(#common::__private::put),
            Self::Patch => quote!(#common::__private::patch),
            Self::Delete => quote!(#common::__private::delete),
        }
    }
}

fn make_future_send(method: &mut TraitItemFn) {
    let output: Type = match &method.sig.output {
        ReturnType::Default => parse_quote!(()),
        ReturnType::Type(_, output) => (**output).clone(),
    };
    method.sig.asyncness = None;
    method.sig.output = parse_quote!(
        -> impl ::core::future::Future<Output = #output> + ::core::marker::Send
    );
}

fn route_verb(attribute: &Attribute) -> Option<&'static str> {
    route_verb_path(attribute.path())
}

fn route_verb_path(path: &syn::Path) -> Option<&'static str> {
    let ident = path.segments.last()?.ident.to_string();
    VERBS.iter().copied().find(|verb| ident == *verb)
}

fn cfg_attr_contains_route_verb(attribute: &Attribute) -> syn::Result<bool> {
    let nested = attribute.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
    Ok(nested
        .iter()
        .skip(1)
        .any(|meta| route_verb_path(meta.path()).is_some()))
}

fn parse_route_path(attribute: &Attribute) -> syn::Result<LitStr> {
    match &attribute.meta {
        Meta::List(list) => {
            let paths = list.parse_args_with(Punctuated::<LitStr, Token![,]>::parse_terminated)?;
            if paths.len() == 1 {
                Ok(paths
                    .first()
                    .expect("a single parsed path must be present")
                    .clone())
            } else {
                Err(Error::new(
                    list.span(),
                    "route attributes require exactly one string path",
                ))
            }
        }
        _ => Err(Error::new(
            attribute.span(),
            "route attributes require exactly one string path",
        )),
    }
}

fn join_paths(prefix: &LitStr, path: &LitStr) -> syn::Result<LitStr> {
    let prefix = prefix.value();
    let path_value = path.value();
    let full_path = if prefix.is_empty() || prefix == "/" {
        path_value
    } else if path_value == "/" {
        prefix
    } else {
        format!("{prefix}{path_value}")
    };
    Ok(LitStr::new(&full_path, path.span()))
}

fn validate_path(path: &LitStr, subject: &str, is_prefix: bool) -> syn::Result<()> {
    let value = path.value();
    if value.is_empty() || !value.starts_with('/') {
        return Err(Error::new(
            path.span(),
            format!("{subject} must be non-empty and start with `/`"),
        ));
    }
    if value.contains(['?', '#']) {
        return Err(Error::new(
            path.span(),
            format!("{subject} must not contain a query string or fragment"),
        ));
    }
    if value.chars().any(char::is_control) {
        return Err(Error::new(
            path.span(),
            format!("{subject} must not contain control characters"),
        ));
    }
    if value.contains(['\\', '%']) || value.chars().any(char::is_whitespace) {
        return Err(Error::new(
            path.span(),
            format!("{subject} must not contain backslashes, percent-encoding, or whitespace"),
        ));
    }
    if value != "/" && value.ends_with('/') {
        return Err(Error::new(
            path.span(),
            format!("{subject} must not end with `/`; use `/` only for the root route"),
        ));
    }

    if value == "/" {
        return Ok(());
    }

    let mut parameters = BTreeSet::new();
    for segment in value.split('/').skip(1) {
        if segment.is_empty() || matches!(segment, "." | "..") {
            return Err(Error::new(
                path.span(),
                format!("{subject} must not contain empty, `.` or `..` segments"),
            ));
        }
        if segment.starts_with('*') || segment.contains(['{', '}']) {
            return Err(Error::new(
                path.span(),
                format!(
                    "{subject} must not use Axum wildcard or brace-capture syntax; use `:parameter` captures"
                ),
            ));
        }
        if let Some(parameter) = segment.strip_prefix(':') {
            let mut characters = parameter.chars();
            let Some(first) = characters.next() else {
                return Err(Error::new(
                    path.span(),
                    format!("{subject} contains an empty parameter"),
                ));
            };
            if !(first == '_' || first.is_ascii_alphabetic())
                || !characters
                    .all(|character| character == '_' || character.is_ascii_alphanumeric())
            {
                return Err(Error::new(
                    path.span(),
                    format!("{subject} parameters must use `[A-Za-z_][A-Za-z0-9_]*`"),
                ));
            }
            if !parameters.insert(parameter) {
                return Err(Error::new(
                    path.span(),
                    format!("{subject} must not repeat parameter `:{parameter}`"),
                ));
            }
        } else if segment.contains(':') {
            return Err(Error::new(
                path.span(),
                format!("{subject} parameters must occupy an entire path segment"),
            ));
        }
    }

    if is_prefix && value.contains(':') {
        return Err(Error::new(
            path.span(),
            "route prefix must not contain parameters; declare them on the endpoint path",
        ));
    }
    Ok(())
}
#[cfg(test)]
#[path = "../tests/support/routes.rs"]
mod tests;

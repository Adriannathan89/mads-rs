//! Procedural macros for compile-time controller and route contracts.
//!
//! The attributes in this crate are re-exported by `mads-common` and, when the
//! `common` feature is enabled, by the `mads` facade. They validate the shape
//! of route traits and controllers during compilation, then emit the static
//! metadata consumed by `mads_common::RouteCatalog` and the MADS dependency
//! graph. Route traits also receive hidden, typed Axum registrars, and each
//! controller descriptor stores a concrete registrar function pointer.
//! Runtime bootstrap validates the complete catalog before any generated
//! registrar is invoked. The generated registrars resolve application-scoped
//! controllers once and invoke handler trait methods through typed Rust calls,
//! never through handler-name metadata.

#![deny(missing_docs)]
#![forbid(unsafe_code)]

use proc_macro::TokenStream;

mod controller;
mod guard;
mod input;
mod passport_principal;
#[cfg(feature = "passport")]
mod passport_strategy;
mod path;
mod routes;
mod verb;

/// Derives deterministic validation without changing Serde deserialization.
///
/// Use `#[validate(email)]`, length and numeric bounds, `nested`, or synchronous
/// custom callbacks on fields. A complete input supports a custom callback.
#[proc_macro_derive(Input, attributes(validate))]
pub fn input(input: TokenStream) -> TokenStream {
    syn::parse(input)
        .and_then(input::expand)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Derives role and permission membership for a named Passport principal.
///
/// Mark at most one collection field with `#[roles]` and at most one with
/// `#[permissions]`. Collection items must implement `AsRef<str>`.
#[proc_macro_derive(PassportPrincipal, attributes(roles, permissions))]
pub fn passport_principal(input: TokenStream) -> TokenStream {
    syn::parse(input)
        .and_then(passport_principal::expand)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Registers a managed, typed Passport JWT strategy implementation.
#[cfg(feature = "passport")]
#[proc_macro_attribute]
pub fn passport_strategy(arguments: TokenStream, item: TokenStream) -> TokenStream {
    passport_strategy::expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares a managed controller and the route traits it must implement.
///
/// The attribute accepts one argument in the form
/// `routes = [RouteTrait, ...]`. The annotated item must be a non-generic
/// named-field or unit struct. Every listed trait must be implemented by the
/// controller; otherwise compilation fails at the controller declaration.
/// Named fields are treated as dependency edges and are resolved by the MADS
/// construction context when the controller is built. The generated public
/// handle is cheap to clone because its state is stored behind an `Arc`. A
/// hidden registrar resolves that handle once and installs every declared
/// route trait through typed dispatch.
///
/// # Examples
///
/// ```rust,ignore
/// #[mads_common::routes]
/// trait HealthRoutes {
///     #[mads_common::get("/health")]
///     async fn health(&self) -> mads_common::HttpResult<&'static str>;
/// }
///
/// #[mads_common::controller(routes = [HealthRoutes])]
/// struct HealthController;
///
/// impl HealthRoutes for HealthController {
///     async fn health(&self) -> mads_common::HttpResult<&'static str> {
///         Ok("ok")
///     }
/// }
/// ```
///
/// The example is marked `ignore` because procedural-macro documentation is
/// compiled in the macro crate itself, while the attributes require a
/// downstream consumer crate and the `mads-common` runtime dependency.
#[proc_macro_attribute]
pub fn controller(arguments: TokenStream, item: TokenStream) -> TokenStream {
    controller::expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares and validates a trait containing HTTP route contracts.
///
/// A route trait may optionally declare a `prefix = "/..."`. It must contain
/// at least one method, and each method must be an async `&self` method with
/// exactly one of the HTTP verb attributes [`macro@get`], [`macro@post`],
/// [`macro@put`], [`macro@patch`], or [`macro@delete`]. Methods remain abstract
/// so that the controller's implementation is the only handler body.
///
/// The macro rejects malformed or ambiguous paths, duplicate method/path
/// pairs, generic traits, and default method implementations. It also emits
/// static route descriptors for later catalog validation and a hidden typed
/// registrar used after validation succeeds.
///
/// # Examples
///
/// ```rust,ignore
/// #[mads_common::routes(prefix = "/users")]
/// trait UserRoutes {
///     #[mads_common::get("/:id")]
///     async fn get_user(
///         &self,
///         id: mads_common::Path<u64>,
///     ) -> mads_common::HttpResult<mads_common::Json<User>>;
/// }
/// # struct User;
/// ```
///
/// The example is marked `ignore` for the same downstream-consumer reason as
/// [`macro@controller`].
#[proc_macro_attribute]
pub fn routes(arguments: TokenStream, item: TokenStream) -> TokenStream {
    routes::expand(arguments.into(), item.into())
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Declares an inheritable Passport policy on a route trait or route method.
///
/// `#[routes]` consumes valid guard attributes and emits their effective
/// static metadata. Applying this attribute to any other item produces a
/// focused diagnostic.
#[proc_macro_attribute]
pub fn guard(arguments: TokenStream, item: TokenStream) -> TokenStream {
    guard::outside_contract(arguments.into(), item.into()).into()
}

/// Marks a GET method inside a trait annotated with [`macro@routes`].
///
/// The attribute takes exactly one string path, such as `#[get("/:id")]`.
/// It is only valid on an abstract async route-contract method; using it on a
/// free function, an inherent method, or a trait without [`macro@routes`]
/// produces a compile-time diagnostic.
///
/// # Examples
///
/// ```rust,ignore
/// #[mads_common::routes]
/// trait HealthRoutes {
///     #[mads_common::get("/health")]
///     async fn health(&self);
/// }
/// ```
#[proc_macro_attribute]
pub fn get(arguments: TokenStream, item: TokenStream) -> TokenStream {
    verb::outside_contract("get", arguments.into(), item.into()).into()
}

/// Marks a POST method inside a trait annotated with [`macro@routes`].
///
/// The attribute takes exactly one string path and is validated together with
/// the route trait's optional prefix.
#[proc_macro_attribute]
pub fn post(arguments: TokenStream, item: TokenStream) -> TokenStream {
    verb::outside_contract("post", arguments.into(), item.into()).into()
}

/// Marks a PUT method inside a trait annotated with [`macro@routes`].
///
/// The attribute takes exactly one string path and is validated together with
/// the route trait's optional prefix.
#[proc_macro_attribute]
pub fn put(arguments: TokenStream, item: TokenStream) -> TokenStream {
    verb::outside_contract("put", arguments.into(), item.into()).into()
}

/// Marks a PATCH method inside a trait annotated with [`macro@routes`].
///
/// The attribute takes exactly one string path and is validated together with
/// the route trait's optional prefix.
#[proc_macro_attribute]
pub fn patch(arguments: TokenStream, item: TokenStream) -> TokenStream {
    verb::outside_contract("patch", arguments.into(), item.into()).into()
}

/// Marks a DELETE method inside a trait annotated with [`macro@routes`].
///
/// The attribute takes exactly one string path and is validated together with
/// the route trait's optional prefix.
#[proc_macro_attribute]
pub fn delete(arguments: TokenStream, item: TokenStream) -> TokenStream {
    verb::outside_contract("delete", arguments.into(), item.into()).into()
}

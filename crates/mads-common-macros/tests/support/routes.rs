//! Unit tests for route-contract expansion.

use super::*;
use quote::ToTokens;

fn method(source: &str) -> TraitItemFn {
    syn::parse_str(source).unwrap_or_else(|error| panic!("{source}: {error}"))
}

fn literal(source: &str) -> LitStr {
    syn::parse_str(source).expect("literal should parse")
}

fn lit(value: &str) -> LitStr {
    LitStr::new(value, proc_macro2::Span::call_site())
}

fn normalized(tokens: impl ToTokens) -> String {
    tokens
        .to_token_stream()
        .to_string()
        .split_whitespace()
        .collect()
}

#[test]
fn expands_a_typed_route_registrar_with_fresh_arguments_and_ufcs_dispatch() {
    let expanded = expand_with_common(
        quote!(prefix = "/users"),
        quote! {
            pub trait UserRoutes {
                #[get("/:id")]
                async fn get_user(
                    &self,
                    user_id: Path<i64>,
                    query: Query<UserQuery>,
                ) -> String;
            }
        },
        &syn::parse_quote!(mads_common),
    )
    .expect("route trait should expand");
    let expanded = normalized(expanded);

    assert!(expanded.contains("fn__mads_register"));
    assert!(expanded.contains(&normalized(quote! {
        __mads_router: mads_common::__private::Router
    })));
    assert!(expanded.contains(&normalized(quote! {
        __mads_runtime: &mads_common::__private::RouterBuildContext<'_>
    })));
    assert!(expanded.contains(&normalized(quote!(mads_common::__private::get))));
    assert!(
        expanded.contains("move|__mads_argument_0:Path<i64>,__mads_argument_1:Query<UserQuery>|{")
    );
    assert!(expanded.contains(&normalized(quote! {
        <Self as UserRoutes>::get_user(
            &__mads_controller,
            __mads_argument_0,
            __mads_argument_1,
        ).await
    })));
    assert!(expanded.contains(&normalized(quote!(
        .with_namespace(module_path!())
    ))));
}

#[test]
fn copies_conditional_attributes_to_method_metadata_and_registration() {
    let expanded = expand_with_common(
        quote!(),
        quote! {
            trait ConditionalRoutes {
                #[cfg(feature = "conditional-route")]
                #[cfg_attr(docsrs, doc(cfg(feature = "conditional-route")))]
                #[get("/conditional")]
                async fn conditional(&self) -> &'static str;
            }
        },
        &syn::parse_quote!(mads_common),
    )
    .expect("conditional route trait should expand");
    let expanded = normalized(expanded);

    assert_eq!(
        expanded
            .matches(&normalized(quote!(#[cfg(feature = "conditional-route")])))
            .count(),
        3,
        "cfg must gate the trait method, descriptor, and registration block",
    );
    assert_eq!(
        expanded
            .matches(&normalized(
                quote!(#[cfg_attr(docsrs, doc(cfg(feature = "conditional-route")))])
            ))
            .count(),
        3,
        "cfg_attr must gate the trait method, descriptor, and registration block",
    );
}

#[test]
fn rejects_route_verbs_nested_in_cfg_attr() {
    let error = expand_with_common(
        quote!(),
        quote! {
            trait ConditionalRoutes {
                #[cfg_attr(feature = "conditional-route", get("/conditional"))]
                async fn conditional(&self) -> &'static str;
            }
        },
        &syn::parse_quote!(mads_common),
    )
    .expect_err("route verbs nested in cfg_attr must be rejected");

    assert_eq!(
        error.to_string(),
        "route verb attributes inside `cfg_attr` are unsupported; use a direct route verb and gate the method with `#[cfg(...)]`",
    );
}

#[test]
fn parses_route_arguments() {
    assert!(matches!(
        syn::parse_str::<RoutesArguments>(""),
        Ok(RoutesArguments { prefix: None })
    ));
    let arguments: RoutesArguments = syn::parse_str("prefix = \"/api\"").unwrap();
    assert_eq!(arguments.prefix.unwrap().value(), "/api");
    assert!(syn::parse_str::<RoutesArguments>("path = \"/api\"").is_err());
    assert!(syn::parse_str::<RoutesArguments>("prefix = \"/api\" extra").is_err());
}

#[cfg(not(feature = "passport"))]
#[test]
fn rejects_guards_when_the_jwt_macro_feature_is_absent() {
    let error = expand_with_common(
        quote!(),
        quote! {
            #[guard(strategy = "jwt", principal = UserPrincipal)]
            trait GuardedRoutes {
                #[get("/profile")]
                async fn profile(&self);
            }
        },
        &syn::parse_quote!(mads_common),
    )
    .expect_err("guards require the macro crate Passport feature");

    assert_eq!(error.to_string(), "guards require the `jwt` feature");
}

#[test]
fn validates_trait_shapes() {
    validate_trait_shape(&syn::parse_str("trait Routes { }").unwrap()).unwrap();
    for source in [
        "trait Routes<T> { }",
        "trait Routes where Self: Sized { }",
        "unsafe trait Routes { }",
        "auto trait Routes { }",
    ] {
        assert!(
            validate_trait_shape(&syn::parse_str(source).unwrap()).is_err(),
            "{source}"
        );
    }
}

#[test]
fn validates_each_supported_http_verb_and_rewrites_async_signature() {
    let mut routes = BTreeSet::new();
    for (verb, name) in [
        ("get", "get_route"),
        ("post", "post_route"),
        ("put", "put_route"),
        ("patch", "patch_route"),
        ("delete", "delete_route"),
    ] {
        let mut route = method(&format!("#[{verb}(\"/\")] async fn {name}(&self) -> i32;"));
        let metadata = validate_method(&mut route, &mut routes, &lit("/api")).unwrap();
        assert_eq!(metadata.full_path.value(), "/api");
        assert!(route.sig.asyncness.is_none());
        assert!(
            route
                .sig
                .output
                .to_token_stream()
                .to_string()
                .contains("Send")
        );
        assert_eq!(
            metadata
                .method
                .tokens(&syn::parse_quote!(common))
                .to_string(),
            format!(
                "common :: HttpMethod :: {}",
                match verb {
                    "get" => "Get",
                    "post" => "Post",
                    "put" => "Put",
                    "patch" => "Patch",
                    _ => "Delete",
                }
            )
        );
        assert_eq!(
            metadata
                .method
                .routing_tokens(&syn::parse_quote!(common))
                .to_string(),
            format!("common :: __private :: {verb}")
        );
    }
}

#[test]
fn rejects_invalid_route_method_contracts() {
    let cases = [
        (
            "#[get(\"/\")] async fn route(&self) {}",
            "default implementations",
        ),
        ("#[get(\"/\")] fn route(&self);", "must be async"),
        (
            "#[get(\"/\")] async unsafe fn route(&self);",
            "cannot be const",
        ),
        ("#[get(\"/\")] async fn route();", "require `&self`"),
        (
            "#[get(\"/\")] async fn route(&mut self);",
            "immutable `&self`",
        ),
        (
            "#[get(\"/\")] async fn route(self: &Self);",
            "immutable `&self`",
        ),
        ("async fn route(&self);", "exactly one"),
        (
            "#[get(\"/\")] #[post(\"/\")] async fn route(&self);",
            "exactly one",
        ),
        ("#[get] async fn route(&self);", "exactly one string path"),
        (
            "#[get(\"/\", \"/other\")] async fn route(&self);",
            "exactly one string path",
        ),
    ];
    for (source, message) in cases {
        let mut routes = BTreeSet::new();
        let error = validate_method(&mut method(source), &mut routes, &lit("/api"))
            .err()
            .expect("route contract must fail");
        assert!(error.to_string().contains(message), "{source}: {error}");
    }
}

#[test]
fn rejects_duplicate_verb_and_path() {
    let mut routes = BTreeSet::new();
    validate_method(
        &mut method("#[get(\"/users\")] async fn first(&self);"),
        &mut routes,
        &lit("/api"),
    )
    .unwrap();
    let error = validate_method(
        &mut method("#[get(\"/users\")] async fn second(&self);"),
        &mut routes,
        &lit("/api"),
    )
    .err()
    .expect("duplicate route should fail");
    assert!(error.to_string().contains("duplicate HTTP verb"));
}

#[test]
fn recognizes_only_qualified_standard_body_extractors() {
    let common = syn::parse_quote!(framework::common);
    for source in [
        "framework::Json<User>",
        "framework::common::Json<User>",
        "framework::ValidatedJson<User>",
        "framework::common::ValidatedJson<User>",
        "framework::Request",
        "framework::common::Request",
        "axum::extract::Request",
        "framework::axum::Json<User>",
    ] {
        let ty = syn::parse_str(source).expect("body extractor type should parse");
        assert!(known_body_consumer(&ty, &common), "{source}");
    }

    for source in [
        "Json<User>",
        "Request",
        "application::Json<User>",
        "mads::Json<User>",
    ] {
        let ty = syn::parse_str(source).expect("custom extractor type should parse");
        assert!(!known_body_consumer(&ty, &common), "{source}");
    }
}

#[test]
fn validates_path_rules_and_prefix_joining() {
    for source in ["\"/\"", "\"/users/:id\"", "\"/users/_id-1\""] {
        validate_path(&literal(source), "route path", false).unwrap();
    }
    let invalid = [
        ("\"\"", false),
        ("\"users\"", false),
        ("\"/users?q=1\"", false),
        ("\"/users#fragment\"", false),
        ("\"/users\\0check\"", false),
        ("\"/users\\\\id\"", false),
        ("\"/users%20id\"", false),
        ("\"/users id\"", false),
        ("\"/users/\"", false),
        ("\"/users//id\"", false),
        ("\"/users/.\"", false),
        ("\"/users/..\"", false),
        ("\"/users/:\"", false),
        ("\"/users/:1id\"", false),
        ("\"/users/:id/:id\"", false),
        ("\"/users/id:name\"", false),
        ("\"/users/:id\"", true),
    ];
    for (source, is_prefix) in invalid {
        assert!(
            validate_path(&literal(source), "route path", is_prefix).is_err(),
            "{source}"
        );
    }
    assert_eq!(
        join_paths(&lit(""), &lit("/users")).unwrap().value(),
        "/users"
    );
    assert_eq!(
        join_paths(&lit("/"), &lit("/users")).unwrap().value(),
        "/users"
    );
    assert_eq!(join_paths(&lit("/api"), &lit("/")).unwrap().value(), "/api");
    assert_eq!(
        join_paths(&lit("/api"), &lit("/users")).unwrap().value(),
        "/api/users"
    );
}

#[test]
fn rejects_axum_reserved_and_malformed_capture_syntax_before_expansion() {
    for path in ["/*rest", "/{id}", "/{id", "/id}", "/users/{id}"] {
        let item: TokenStream = syn::parse_str(&format!(
            "trait InvalidRoutes {{ #[get(\"{path}\")] async fn route(&self); }}"
        ))
        .expect("route trait should parse");
        assert!(
            expand_with_common(quote!(), item, &syn::parse_quote!(mads_common)).is_err(),
            "macro expansion accepted reserved path `{path}`"
        );
    }

    for prefix in ["/*rest", "/{id}", "/{id", "/id}"] {
        let arguments: TokenStream = syn::parse_str(&format!("prefix = \"{prefix}\""))
            .expect("route arguments should parse");
        assert!(
            expand_with_common(
                arguments,
                quote! {
                    trait InvalidPrefixRoutes {
                        #[get("/users")]
                        async fn route(&self);
                    }
                },
                &syn::parse_quote!(mads_common),
            )
            .is_err(),
            "macro expansion accepted reserved prefix `{prefix}`"
        );
    }
}

#[test]
fn identifies_route_verbs_and_parses_attributes() {
    let item: syn::ItemFn = syn::parse_str("#[get(\"/users\")] fn list() {}").unwrap();
    assert_eq!(route_verb(&item.attrs[0]), Some("get"));
    let unknown: syn::ItemFn = syn::parse_str("#[trace(\"/users\")] fn list() {}").unwrap();
    assert_eq!(route_verb(&unknown.attrs[0]), None);
    assert_eq!(parse_route_path(&item.attrs[0]).unwrap().value(), "/users");
}

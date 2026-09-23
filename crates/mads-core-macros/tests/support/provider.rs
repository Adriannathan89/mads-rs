#[cfg(test)]
    mod tests {
        use super::*;
        use quote::ToTokens;

        fn function(source: &str) -> ItemFn {
            syn::parse_str(source).expect("test function should parse")
        }

        fn ty(source: &str) -> Type {
            syn::parse_str(source).expect("test type should parse")
        }

        #[test]
        fn validates_provider_signature_constraints() {
            let cases = [
                (
                    "fn method(&self) -> i32 { 1 }",
                    "cannot be applied to methods",
                ),
                ("fn generic<T>() -> i32 { 1 }", "does not support lifetime"),
                (
                    "fn missing() { }",
                    "requires an explicit concrete return type",
                ),
                (
                    "fn inferred() -> _ { 1 }",
                    "requires an explicit concrete return type",
                ),
                (
                    "fn opaque() -> impl Copy { 1 }",
                    "requires an explicit concrete return type",
                ),
                (
                    "unsafe fn unsafe_provider() -> i32 { 1 }",
                    "does not support unsafe",
                ),
            ];
            for (source, message) in cases {
                let error = validate_signature(&function(source)).expect_err("signature must fail");
                assert!(error.to_string().contains(message), "{source}: {error}");
            }
        }

        #[test]
        fn provider_expansion_records_the_declaration_namespace() {
            let item = function("pub fn value() -> i32 { 1 }");
            let expanded = expand_provider_with_core(item, syn::parse_quote!(mads_core))
                .expect("provider should expand")
                .to_string();

            assert!(
                expanded.contains(". with_namespace (module_path ! ())"),
                "expanded descriptor did not record its namespace: {expanded}"
            );
        }

        #[test]
        fn recognizes_fallible_output_forms() {
            for source in [
                "Result<i32>",
                "mads_core::Result<i32>",
                "mads::core::Result<i32>",
            ] {
                let return_type = ty(source);
                let output =
                    result_output(&return_type).expect("result output should be recognized");
                assert_eq!(output.to_token_stream().to_string(), "i32");
            }
        }

        #[test]
        fn rejects_non_result_or_malformed_result_types() {
            for source in [
                "i32",
                "Other::Result<i32>",
                "mads::Result<i32>",
                "Result",
                "Result<i32, String>",
                "Result<'static>",
                "<T as Trait>::Result<i32>",
            ] {
                assert!(
                    result_output(&ty(source)).is_none(),
                    "{source} must not be recognized"
                );
            }
        }

        #[test]
        fn unwraps_grouped_and_parenthesized_types() {
            assert_eq!(
                ungroup_type(&ty("(i32)")).to_token_stream().to_string(),
                "i32"
            );
            assert_eq!(
                ungroup_type(&ty("((i32))")).to_token_stream().to_string(),
                "i32"
            );
        }

        #[test]
        fn finds_inferred_types_and_const_expressions() {
            assert!(non_concrete_output_span(&ty("_")).is_some());
            assert!(non_concrete_output_span(&ty("impl Iterator<Item = i32>")).is_some());
            assert!(non_concrete_output_span(&ty("Vec<_>")).is_some());
            assert!(non_concrete_output_span(&ty("Array<{ _ }>")).is_some());
            assert!(non_concrete_output_span(&ty("Vec<i32>")).is_none());
        }

        #[test]
        fn expansion_rejects_arguments_before_resolving_paths() {
            let error = expand(
                quote!(unexpected),
                quote!(
                    fn value() -> i32 {
                        1
                    }
                ),
            )
            .expect_err("provider arguments must be rejected");
            assert!(error.to_string().contains("provider(lifecycle)"));
        }

        #[test]
        fn parses_only_ordinary_and_lifecycle_modes() {
            assert_eq!(parse_provider_mode(TokenStream::new()).unwrap(), ProviderMode::Ordinary);
            assert_eq!(
                parse_provider_mode(quote!(lifecycle)).unwrap(),
                ProviderMode::Lifecycle
            );
            for arguments in [quote!(other), quote!(lifecycle, other), quote!(lifecycle lifecycle)] {
                let error = parse_provider_mode(arguments).expect_err("arguments must fail");
                assert!(error.to_string().contains("provider(lifecycle)"));
            }
        }

        #[test]
        fn recognizes_only_approved_lifecycle_outputs() {
            for source in [
                "LifecycleResource<NativeResource>",
                "mads_core::LifecycleResource<NativeResource>",
            ] {
                let return_type = ty(source);
                let (output, fallible) = lifecycle_resource_output(&return_type).unwrap();
                assert_eq!(output.to_token_stream().to_string(), "NativeResource");
                assert!(!fallible);
            }

            let return_type = ty("mads::core::Result<LifecycleResource<NativeResource>>");
            let (output, fallible) = lifecycle_resource_output(&return_type).unwrap();
            assert_eq!(output.to_token_stream().to_string(), "NativeResource");
            assert!(fallible);

            for source in [
                "NativeResource",
                "Option<LifecycleResource<NativeResource>>",
                "std::result::Result<LifecycleResource<NativeResource>, CustomError>",
                "LifecycleResource<NativeResource, Other>",
            ] {
                assert!(lifecycle_resource_output(&ty(source)).is_none(), "{source}");
            }
        }

        #[test]
        fn lifecycle_mode_requires_async_and_approved_output() {
            for source in [
                "fn resource() -> LifecycleResource<NativeResource> { todo!() }",
                "async fn resource() -> NativeResource { todo!() }",
            ] {
                let error = validate_lifecycle_signature(&function(source))
                    .expect_err("lifecycle signature must fail");
                assert!(error.to_string().contains("two accepted async forms"));
            }
        }

        #[test]
        fn lifecycle_expansion_registers_native_output_and_both_constructors() {
            let item = function(
                "async fn resource() -> LifecycleResource<NativeResource> { todo!() }",
            );
            let expanded = expand_lifecycle_provider_with_core(
                item,
                syn::parse_quote!(mads_core),
            )
            .expect("lifecycle provider should expand")
            .to_string();

            assert!(expanded.contains("TypeId :: of :: < NativeResource >"), "{expanded}");
            assert!(
                !expanded.contains("TypeId :: of :: < LifecycleResource < NativeResource > >"),
                "{expanded}"
            );
            assert!(
                expanded.contains(
                    ". with_runtime_type_name (__mads_runtime_type_name_resource) . with_lifecycle_constructor (__mads_construct_lifecycle_resource)"
                ),
                "{expanded}"
            );
            assert!(expanded.contains("fn __mads_construct < 'a >"), "{expanded}");
        }
    }

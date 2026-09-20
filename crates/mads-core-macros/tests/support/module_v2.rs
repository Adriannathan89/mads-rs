#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_global_module_with_imports() {
        let arguments: ModuleArguments = syn::parse2(quote::quote!(
            global,
            imports = [crate::database::DatabaseModule],
        ))
        .expect("global module arguments should parse");

        let imports: Vec<_> = arguments
            .imports
            .iter()
            .map(quote::ToTokens::to_token_stream)
            .map(|path| path.to_string())
            .collect();
        assert_eq!(imports, ["crate :: database :: DatabaseModule"]);
    }

    #[test]
    fn rejects_unknown_and_repeated_module_arguments() {
        for (arguments, expected_error) in [
            (
                quote::quote!(providers = [DatabasePool]),
                "`#[mads::module]` supports only unit structs with optional `imports = [Module]` or `global`",
            ),
            (
                quote::quote!(global, global),
                "duplicate `global` argument",
            ),
            (
                quote::quote!(imports = [A], imports = [B]),
                "duplicate `imports` argument",
            ),
        ] {
            let error = syn::parse2::<ModuleArguments>(arguments)
                .err()
                .expect("invalid module arguments must be rejected");

            assert_eq!(error.to_string(), expected_error);
        }
    }
}

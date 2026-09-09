//! Serde representation metadata used only for canonical issue paths.

use syn::{Attribute, Error, LitStr, Meta, Token, punctuated::Punctuated};

#[derive(Default)]
pub(super) struct Naming {
    pub rename: Option<String>,
    pub rename_all: Option<String>,
    pub rename_all_fields: Option<String>,
    pub flatten: bool,
    pub untagged: bool,
    pub tag: Option<String>,
    pub content: Option<String>,
}

impl Naming {
    pub fn parse(attributes: &[Attribute]) -> syn::Result<Self> {
        let mut naming = Self::default();
        for attr in attributes
            .iter()
            .filter(|attr| attr.path().is_ident("serde"))
        {
            // Parse all metadata structurally, ignoring deserialization behavior
            // such as defaults and aliases: Serde owns those semantics.
            let items = attr.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)?;
            for item in items {
                let path = item.path();
                if path.is_ident("flatten") {
                    naming.flatten = true;
                } else if path.is_ident("untagged") {
                    naming.untagged = true;
                } else if path.is_ident("rename") {
                    naming.rename = deserialize_name(&item)?;
                } else if path.is_ident("rename_all") {
                    naming.rename_all = deserialize_name(&item)?;
                } else if path.is_ident("rename_all_fields") {
                    naming.rename_all_fields = deserialize_name(&item)?;
                } else if path.is_ident("tag") {
                    naming.tag = deserialize_name(&item)?;
                } else if path.is_ident("content") {
                    naming.content = deserialize_name(&item)?;
                }
            }
        }
        Ok(naming)
    }
}

fn deserialize_name(meta: &Meta) -> syn::Result<Option<String>> {
    match meta {
        Meta::NameValue(value) => {
            let expression = &value.value;
            let literal: LitStr = syn::parse2(quote::quote!(#expression))?;
            Ok(Some(literal.value()))
        }
        Meta::List(list) => {
            let mut result = None;
            list.parse_nested_meta(|meta| {
                let value: LitStr = meta.value()?.parse()?;
                if meta.path.is_ident("deserialize") {
                    result = Some(value.value());
                }
                Ok(())
            })?;
            Ok(result)
        }
        _ => Err(Error::new_spanned(meta, "expected a Serde name")),
    }
}

pub(super) fn rename(name: &str, rule: Option<&str>, variant: bool) -> syn::Result<String> {
    let name = name.strip_prefix("r#").unwrap_or(name);
    let Some(rule) = rule else {
        return Ok(name.into());
    };
    let snake = || {
        if !variant {
            return name.to_owned();
        }
        let mut output = String::new();
        for (index, character) in name.char_indices() {
            if index > 0 && character.is_uppercase() {
                output.push('_');
            }
            output.push(character.to_ascii_lowercase());
        }
        output
    };
    let pascal = || {
        if variant {
            return name.to_owned();
        }
        let mut upper = true;
        let mut output = String::new();
        for character in name.chars() {
            if character == '_' {
                upper = true;
            } else {
                output.push(if upper {
                    character.to_ascii_uppercase()
                } else {
                    character
                });
                upper = false;
            }
        }
        output
    };
    Ok(match rule {
        "lowercase" => {
            if variant {
                name.to_ascii_lowercase()
            } else {
                name.into()
            }
        }
        "UPPERCASE" => name.to_ascii_uppercase(),
        "PascalCase" => pascal(),
        "camelCase" => {
            let value = pascal();
            let mut chars = value.chars();
            chars.next().map_or(String::new(), |first| {
                format!("{}{}", first.to_ascii_lowercase(), chars.as_str())
            })
        }
        "snake_case" => snake(),
        "SCREAMING_SNAKE_CASE" => snake().to_ascii_uppercase(),
        "kebab-case" => snake().replace('_', "-"),
        "SCREAMING-KEBAB-CASE" => snake().to_ascii_uppercase().replace('_', "-"),
        _ => {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                "unknown Serde rename rule",
            ));
        }
    })
}

//! Typed configuration derive grammar and ordered field construction.

use std::collections::BTreeSet;

use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::{
    Data, DeriveInput, Error, Expr, Fields, GenericArgument, Generics, Lit, LitStr, PathArguments,
    Type, TypeParamBound, WherePredicate, spanned::Spanned,
};

use crate::path::core_path;

#[path = "configuration/validate.rs"]
mod validate;

#[derive(Default)]
struct Attributes {
    rename: Option<LitStr>,
    default: Option<Expr>,
    parser: Option<syn::Path>,
    validators: Option<Vec<validate::Validator>>,
}

pub(crate) fn expand(input: DeriveInput) -> syn::Result<TokenStream> {
    let core = core_path()?;
    // Rustdoc compiles examples as separate crates while package discovery can
    // still report Itself. The runtime's self-alias also supports internal derives.
    let core: syn::Path = if core.is_ident("crate") {
        syn::parse_quote!(::mads_core)
    } else {
        core
    };
    let helper = quote!(#core::__private::configuration);
    let mut prefix = None;
    for attribute in input
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("config"))
    {
        attribute.parse_nested_meta(|meta| {
            if !meta.path.is_ident("prefix") || prefix.is_some() {
                return Err(meta.error("expected one configuration prefix"));
            }
            let value: LitStr = meta.value()?.parse()?;
            validate_key(&value, true)?;
            prefix = Some(value.value());
            Ok(())
        })?;
    }
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            _ => {
                return Err(Error::new(
                    input.ident.span(),
                    "Configuration requires a named-field struct",
                ));
            }
        },
        _ => {
            return Err(Error::new(
                input.ident.span(),
                "Configuration requires a named-field struct",
            ));
        }
    };
    let mut keys = BTreeSet::new();
    let mut reads = Vec::new();
    let mut builds = Vec::new();
    for (index, field) in fields.iter().enumerate() {
        let ident = field.ident.as_ref().expect("named fields");
        let attributes = field_attributes(field)?;
        let key = attributes
            .rename
            .as_ref()
            .map(LitStr::value)
            .unwrap_or_else(|| ident.to_string().trim_start_matches("r#").to_owned());
        if !keys.insert(key.clone()) {
            return Err(Error::new(
                attributes
                    .rename
                    .as_ref()
                    .map_or(ident.span(), LitStr::span),
                "configuration field key collides with another field",
            ));
        }
        let key = match prefix.as_deref() {
            Some(prefix) if !prefix.is_empty() => format!("{prefix}.{key}"),
            _ => key,
        };
        let value = field_read(
            &field.ty,
            &attributes,
            &input.generics,
            &core,
            &helper,
            &key,
        )?;
        let local = format_ident!("__mads_field_{index}");
        let ty = &field.ty;
        let validation = validate::expand(
            ty,
            attributes.validators.as_deref().unwrap_or(&[]),
            &core,
            &key,
        )?;
        reads.push(quote_spanned! {field.span()=>
            let #local: ::core::option::Option<#ty> = #helper::collect(
                (#value).and_then(|__mads_value: #ty| {
                    #validation
                    Ok(__mads_value)
                }), &mut __mads_errors,
            );
        });
        builds.push(quote!(#ident: #local.expect("successful configuration field")));
    }
    let name = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();
    Ok(quote! {
        impl #impl_generics #core::Configuration for #name #ty_generics #where_clause {
            fn from_config(__mads_config: &#core::Config) -> #core::ConfigurationResult<Self> {
                let mut __mads_errors: ::core::option::Option<#core::ConfigurationErrors> = None;
                #(#reads)*
                if let Some(errors) = __mads_errors {
                    return Err(errors);
                }
                Ok(Self { #(#builds),* })
            }
        }
    })
}

fn field_attributes(field: &syn::Field) -> syn::Result<Attributes> {
    let mut result = Attributes::default();
    for attribute in field
        .attrs
        .iter()
        .filter(|attr| attr.path().is_ident("config"))
    {
        attribute.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") && result.rename.is_none() {
                let value: LitStr = meta.value()?.parse()?;
                validate_key(&value, false)?;
                result.rename = Some(value);
            } else if meta.path.is_ident("default") && result.default.is_none() {
                result.default = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("parse_with") && result.parser.is_none() {
                result.parser = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("validate") && result.validators.is_none() {
                result.validators = Some(validate::parse(meta)?);
            } else {
                return Err(meta.error("unknown or duplicate configuration field attribute"));
            }
            Ok(())
        })?;
    }
    Ok(result)
}

fn validate_key(value: &LitStr, allow_empty: bool) -> syn::Result<()> {
    let key = value.value();
    if key.contains('.') || (!allow_empty && key.is_empty()) {
        return Err(Error::new(
            value.span(),
            "configuration keys must be single nonempty segments",
        ));
    }
    Ok(())
}

fn type_name(ty: &Type) -> syn::Result<String> {
    match ty {
        Type::Path(path) if path.qself.is_none() => {
            Ok(path.path.segments.last().unwrap().ident.to_string())
        }
        _ => Err(Error::new(
            ty.span(),
            "unsupported configuration field shape",
        )),
    }
}

fn inner_type(ty: &Type) -> syn::Result<&Type> {
    if let Type::Path(path) = ty {
        if let PathArguments::AngleBracketed(arguments) =
            &path.path.segments.last().unwrap().arguments
        {
            if arguments.args.len() == 1 {
                if let Some(GenericArgument::Type(inner)) = arguments.args.first() {
                    return Ok(inner);
                }
            }
        }
    }
    Err(Error::new(
        ty.span(),
        "expected one configuration value type",
    ))
}

fn is_scalar(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "bool"
            | "char"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
    )
}

fn field_read(
    ty: &Type,
    attributes: &Attributes,
    generics: &Generics,
    core: &syn::Path,
    helper: &TokenStream,
    key: &str,
) -> syn::Result<TokenStream> {
    let name = type_name(ty)?;
    if name == "Option" {
        if attributes.default.is_some() {
            return Err(Error::new(
                ty.span(),
                "optional configuration fields cannot declare defaults",
            ));
        }
        let inner = inner_type(ty)?;
        if type_name(inner)? == "Option" {
            return Err(Error::new(
                inner.span(),
                "nested Option configuration fields are unsupported",
            ));
        }
        let read = field_read(inner, attributes, generics, core, helper, key)?;
        return Ok(quote! {
            if #helper::present(__mads_config, #key) {
                (#read).map(Some)
            } else {
                Ok(None)
            }
        });
    }
    if name == "Secret" {
        if attributes.default.is_some() {
            return Err(Error::new(
                ty.span(),
                "secret configuration fields cannot declare defaults",
            ));
        }
        let inner = inner_type(ty)?;
        let inner_name = type_name(inner)?;
        if attributes.parser.is_none() && !is_scalar(&inner_name) && inner_name != "PathBuf" {
            return Err(Error::new(
                inner.span(),
                "Secret requires a supported scalar configuration type",
            ));
        }
        let read = field_read(inner, attributes, generics, core, helper, key)?;
        return Ok(quote!((#read).map(#core::Secret::new)));
    }
    let read = if let Some(parser) = &attributes.parser {
        quote_spanned! {parser.span()=> {
            let __mads_parser: fn(&str) -> ::core::result::Result<#ty, _> = #parser;
            #helper::parse_with(__mads_config, #key, __mads_parser)
        }}
    } else if is_scalar(&name) {
        quote!(#helper::scalar::<#ty>(__mads_config, #key))
    } else if name == "PathBuf" {
        quote!(#helper::path(__mads_config, #key))
    } else if name == "Vec" {
        if type_name(inner_type(ty)?)? != "String" {
            return Err(Error::new(
                ty.span(),
                "configuration arrays support only Vec<String>",
            ));
        }
        quote!(#helper::string_array(__mads_config, #key))
    } else {
        if matches!(
            name.as_str(),
            "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet" | "Box" | "Result"
        ) {
            return Err(Error::new(
                ty.span(),
                "unsupported configuration field shape",
            ));
        }
        if generics
            .type_params()
            .any(|parameter| parameter.ident == name)
            && !configuration_bound(&name, generics)
        {
            return Err(Error::new(
                ty.span(),
                "generic configuration fields require a Configuration bound or parse_with",
            ));
        }
        quote_spanned! {ty.span()=> {
            fn __mads_read_nested_configuration<T: #core::Configuration>(
                __mads_config: &#core::Config,
                __mads_key: &str,
            ) -> #core::ConfigurationResult<T> {
                #helper::nested::<T>(__mads_config, __mads_key)
            }
            __mads_read_nested_configuration::<#ty>(__mads_config, #key)
        }}
    };
    if let Some(default) = &attributes.default {
        let value = default_value(default, &name)?;
        return Ok(quote! {
            if #helper::present(__mads_config, #key) {
                #read
            } else {
                Ok(#value)
            }
        });
    }
    Ok(read)
}

fn configuration_bound(name: &str, generics: &Generics) -> bool {
    let matches_bound = |bound: &TypeParamBound| {
        matches!(bound, TypeParamBound::Trait(bound)
            if bound.path.segments.last().is_some_and(|segment| segment.ident == "Configuration"))
    };
    generics
        .type_params()
        .any(|parameter| parameter.ident == name && parameter.bounds.iter().any(matches_bound))
        || generics.where_clause.as_ref().is_some_and(|clause| {
            clause.predicates.iter().any(|predicate| {
                matches!(predicate, WherePredicate::Type(predicate)
                if type_name(&predicate.bounded_ty).ok().as_deref() == Some(name)
                    && predicate.bounds.iter().any(matches_bound))
            })
        })
}

fn default_value(expression: &Expr, name: &str) -> syn::Result<TokenStream> {
    let literal = match expression {
        Expr::Lit(literal) => &literal.lit,
        Expr::Unary(unary) if matches!(unary.op, syn::UnOp::Neg(_)) => match unary.expr.as_ref() {
            Expr::Lit(literal) if matches!(literal.lit, Lit::Int(_) | Lit::Float(_)) => {
                &literal.lit
            }
            _ => {
                return Err(Error::new(
                    expression.span(),
                    "configuration defaults must be compatible literals",
                ));
            }
        },
        _ => {
            return Err(Error::new(
                expression.span(),
                "configuration defaults must be compatible literals",
            ));
        }
    };
    let compatible = match literal {
        Lit::Str(_) => name == "String",
        Lit::Bool(_) => name == "bool",
        Lit::Char(_) => name == "char",
        Lit::Int(_) => is_scalar(name) && !matches!(name, "String" | "bool" | "char"),
        Lit::Float(_) => matches!(name, "f32" | "f64"),
        _ => false,
    };
    if !compatible {
        return Err(Error::new(
            expression.span(),
            "configuration default is incompatible with the field type",
        ));
    }
    if is_scalar(name) && !matches!(name, "String" | "bool" | "char") {
        validate::check_number(expression, name)?;
    }
    if name == "String" {
        Ok(quote!(::std::string::String::from(#expression)))
    } else if matches!(name, "f32" | "f64") {
        let ty: Type = syn::parse_str(name)?;
        let value = validate::numeric(expression)?;
        Ok(quote!((#value) as #ty))
    } else {
        Ok(quote!(#expression))
    }
}

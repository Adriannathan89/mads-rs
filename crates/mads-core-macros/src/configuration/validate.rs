//! Compatible configuration validator grammar and generated checks.

use proc_macro2::TokenStream;
use quote::quote;
use std::collections::BTreeSet;
use syn::{Error, Expr, Lit, Type, meta::ParseNestedMeta, spanned::Spanned};

pub(super) struct Validator {
    name: String,
    span: proc_macro2::Span,
    arguments: Vec<(String, Expr)>,
}

pub(super) fn parse(meta: ParseNestedMeta<'_>) -> syn::Result<Vec<Validator>> {
    let mut validators = Vec::new();
    let mut names = BTreeSet::new();
    meta.parse_nested_meta(|meta| {
        let name = meta
            .path
            .get_ident()
            .ok_or_else(|| meta.error("expected a validator name"))?
            .to_string();
        if !names.insert(name.clone()) {
            return Err(meta.error("duplicate configuration validator"));
        }
        let mut arguments = Vec::new();
        match name.as_str() {
            "email" | "nonempty" | "positive" | "negative" => {}
            "multiple_of" => arguments.push(("multiple".into(), meta.value()?.parse()?)),
            "length" | "range" => {
                let mut keys = BTreeSet::new();
                meta.parse_nested_meta(|argument| {
                    let key = argument
                        .path
                        .get_ident()
                        .ok_or_else(|| argument.error("expected a bound"))?
                        .to_string();
                    if !(matches!(key.as_str(), "min" | "max")
                        || key == "exact" && name == "length")
                    {
                        return Err(argument.error("unknown configuration validator bound"));
                    }
                    if !keys.insert(key.clone()) {
                        return Err(argument.error("duplicate configuration validator bound"));
                    }
                    arguments.push((key, argument.value()?.parse()?));
                    Ok(())
                })?;
                if arguments.is_empty() || (keys.contains("exact") && keys.len() != 1) {
                    return Err(meta.error("expected min/max bounds or one exact length"));
                }
                let min = arguments.iter().find(|(key, _)| key == "min");
                let max = arguments.iter().find(|(key, _)| key == "max");
                if let (Some((_, min)), Some((_, max))) = (min, max) {
                    if compare(min, max)? == std::cmp::Ordering::Greater {
                        return Err(meta.error("minimum configuration bound exceeds maximum"));
                    }
                }
            }
            _ => return Err(meta.error("unknown configuration validator")),
        }
        for (_, argument) in &arguments {
            let value = numeric(argument)?;
            if name == "multiple_of" && value == 0.0 {
                return Err(Error::new(argument.span(), "multiple_of must be nonzero"));
            }
            if name == "length"
                && (value < 0.0
                    || !matches!(argument, Expr::Lit(expr) if matches!(expr.lit, Lit::Int(_))))
            {
                return Err(Error::new(
                    argument.span(),
                    "length bounds must be nonnegative integer literals",
                ));
            }
        }
        validators.push(Validator {
            name,
            span: meta.path.span(),
            arguments,
        });
        Ok(())
    })?;
    if validators.is_empty() {
        return Err(meta.error("expected at least one configuration validator"));
    }
    if names.contains("nonempty") {
        for validator in &validators {
            if validator.name == "length"
                && validator.arguments.iter().any(|(key, value)| {
                    matches!(key.as_str(), "max" | "exact") && numeric(value).ok() == Some(0.0)
                })
            {
                return Err(Error::new(
                    validator.span,
                    "length bound contradicts nonempty",
                ));
            }
        }
    }
    if names.contains("positive") && names.contains("negative") {
        return Err(
            meta.error("positive and negative configuration validators contradict each other")
        );
    }
    Ok(validators)
}

pub(super) fn check_number(expression: &Expr, name: &str) -> syn::Result<()> {
    let (negative, literal) = literal(expression)?;
    let overflow = || {
        Error::new(
            expression.span(),
            "numeric literal is outside the configuration field type",
        )
    };
    if matches!(name, "f32" | "f64") {
        let value = numeric(expression)?;
        if name == "f32" && (!(value as f32).is_finite() || (value != 0.0 && value as f32 == 0.0)) {
            return Err(overflow());
        }
        return Ok(());
    }
    let Lit::Int(value) = literal else {
        return Err(overflow());
    };
    if !value.suffix().is_empty() && value.suffix() != name {
        return Err(Error::new(
            expression.span(),
            "numeric literal suffix does not match the configuration field type",
        ));
    }
    let value = value.base10_parse::<u128>()?;
    let unsigned = name.starts_with('u');
    if unsigned && negative {
        return Err(overflow());
    }
    // Pointer-sized values remain checked by rustc for the consumer's target.
    if let Ok(bits) = name[1..].parse::<u32>() {
        let maximum = if unsigned {
            u128::MAX >> (128 - bits)
        } else {
            (1_u128 << (bits - 1)) - u128::from(!negative)
        };
        if value > maximum {
            return Err(overflow());
        }
    }
    Ok(())
}

pub(super) fn numeric(expression: &Expr) -> syn::Result<f64> {
    let (negative, literal) = literal(expression)?;
    let value = match literal {
        Lit::Int(value) => value.base10_parse::<f64>(),
        Lit::Float(value) => value.base10_parse::<f64>(),
        _ => {
            return Err(Error::new(
                expression.span(),
                "expected a finite numeric literal",
            ));
        }
    }?;
    if !value.is_finite() {
        return Err(Error::new(
            expression.span(),
            "expected a finite numeric literal",
        ));
    }
    Ok(if negative { -value } else { value })
}

fn literal(expression: &Expr) -> syn::Result<(bool, &Lit)> {
    match expression {
        Expr::Lit(value) => Ok((false, &value.lit)),
        Expr::Unary(value) if matches!(value.op, syn::UnOp::Neg(_)) => {
            if let Expr::Lit(value) = value.expr.as_ref() {
                Ok((true, &value.lit))
            } else {
                Err(Error::new(expression.span(), "expected a numeric literal"))
            }
        }
        _ => Err(Error::new(expression.span(), "expected a numeric literal")),
    }
}

fn compare(left: &Expr, right: &Expr) -> syn::Result<std::cmp::Ordering> {
    let (left_negative, left_literal) = literal(left)?;
    let (right_negative, right_literal) = literal(right)?;
    if let (Lit::Int(left), Lit::Int(right)) = (left_literal, right_literal) {
        let left = left.base10_parse::<u128>()?;
        let right = right.base10_parse::<u128>()?;
        let left_negative = left_negative && left != 0;
        let right_negative = right_negative && right != 0;
        return Ok(match (left_negative, right_negative) {
            (true, false) => std::cmp::Ordering::Less,
            (false, true) => std::cmp::Ordering::Greater,
            (false, false) => left.cmp(&right),
            (true, true) => right.cmp(&left),
        });
    }
    Ok(numeric(left)?
        .partial_cmp(&numeric(right)?)
        .expect("finite bounds"))
}

pub(super) fn expand(
    ty: &Type,
    validators: &[Validator],
    core: &syn::Path,
    key: &str,
) -> syn::Result<TokenStream> {
    let mut ty = ty;
    let optional = super::type_name(ty)? == "Option";
    if optional {
        ty = super::inner_type(ty)?;
    }
    let secret = super::type_name(ty)? == "Secret";
    if secret {
        ty = super::inner_type(ty)?;
    }
    let name = super::type_name(ty)?;
    let float = matches!(name.as_str(), "f32" | "f64");
    let number = super::is_scalar(&name) && !matches!(name.as_str(), "String" | "bool" | "char");
    let length = if name == "String" {
        quote!(__mads_checked.chars().count())
    } else {
        quote!(__mads_checked.len())
    };
    let runtime = quote!(#core::__private::configuration_validation);
    let mut checks = Vec::new();
    for validator in validators {
        let valid = match validator.name.as_str() {
            "email" => name == "String",
            "length" | "nonempty" => matches!(name.as_str(), "String" | "Vec"),
            "negative" => number && !name.starts_with('u'),
            _ => number,
        };
        if !valid {
            return Err(Error::new(
                validator.span,
                "configuration validator is incompatible with the field type",
            ));
        }
        if number {
            for (_, value) in &validator.arguments {
                check_number(value, &name)?;
            }
        }
        let mut conditions = Vec::new();
        match validator.name.as_str() {
            "email" => {
                conditions.push((quote!(!#runtime::email(__mads_checked)), "invalid_format"))
            }
            "nonempty" => conditions.push((quote!(#length == 0), "too_small")),
            "positive" => conditions.push((quote!(*__mads_checked <= 0 as #ty), "too_small")),
            "negative" => conditions.push((quote!(*__mads_checked >= 0 as #ty), "too_big")),
            "multiple_of" => {
                let value = &validator.arguments[0].1;
                if float {
                    let divisor = numeric(value)?;
                    conditions.push((
                        quote!(!#runtime::multiple(
                            *__mads_checked as f64, #divisor, <#ty>::EPSILON as f64,
                        )),
                        "not_multiple_of",
                    ));
                } else {
                    conditions.push((
                        quote!(!matches!(__mads_checked.checked_rem(#value), Some(0) | None)),
                        "not_multiple_of",
                    ));
                }
            }
            "length" | "range" => {
                for (bound, value) in &validator.arguments {
                    let value = if float {
                        let value = numeric(value)?;
                        quote!((#value) as #ty)
                    } else {
                        quote!(#value)
                    };
                    let observed = if validator.name == "length" {
                        length.clone()
                    } else {
                        quote!(*__mads_checked)
                    };
                    if bound == "min" || bound == "exact" {
                        conditions.push((quote!(#observed < #value), "too_small"));
                    }
                    if bound == "max" || bound == "exact" {
                        conditions.push((quote!(#observed > #value), "too_big"));
                    }
                }
            }
            _ => unreachable!("parsed validator"),
        }
        for (condition, code) in conditions {
            checks.push(quote! {
                if #condition {
                    #runtime::push(&mut __mads_validation_errors, __mads_config, #key, #code);
                }
            });
        }
    }
    let finite_checks = if float {
        quote! {
            if !__mads_checked.is_finite() {
                #runtime::push(&mut __mads_validation_errors, __mads_config, #key, "invalid_type");
            } else { #(#checks)* }
        }
    } else {
        quote!(#(#checks)*)
    };
    let checked = if secret {
        quote!(__mads_checked.expose())
    } else {
        quote!(__mads_checked)
    };
    let body = quote! { let __mads_checked = #checked; #finite_checks };
    let body = if optional {
        quote! { if let Some(__mads_checked) = &__mads_value { #body } }
    } else {
        quote! { let __mads_checked = &__mads_value; #body }
    };
    Ok(quote! {
        let mut __mads_validation_errors: Option<#core::ConfigurationErrors> = None;
        #body
        if let Some(errors) = __mads_validation_errors { return Err(errors); }
    })
}

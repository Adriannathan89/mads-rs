//! Strict validation grammar and literal checks.

use proc_macro2::Span;
use std::{cmp::Ordering, collections::BTreeSet};
use syn::{Attribute, Error, Expr, Lit, Path, spanned::Spanned};

pub(super) struct Validator {
    pub name: String,
    pub span: Span,
    pub arguments: Vec<(String, Expr)>,
    pub callback: Option<Path>,
}

pub(super) fn parse(attributes: &[Attribute], whole: bool) -> syn::Result<Vec<Validator>> {
    let mut validators = Vec::new();
    let mut names = BTreeSet::new();
    for attribute in attributes
        .iter()
        .filter(|attr| attr.path().is_ident("validate"))
    {
        let before = validators.len();
        attribute.parse_nested_meta(|meta| {
            let name = meta
                .path
                .get_ident()
                .ok_or_else(|| meta.error("expected an input validator"))?
                .to_string();
            if !names.insert(name.clone()) {
                return Err(meta.error("duplicate input validator"));
            }
            if whole && name != "custom" {
                return Err(meta.error("only custom validation is supported on a complete input"));
            }
            let mut validator = Validator {
                name: name.clone(),
                span: meta.path.span(),
                arguments: Vec::new(),
                callback: None,
            };
            match name.as_str() {
                "email" | "nonempty" | "positive" | "negative" | "required" | "nested" => {}
                "custom" => validator.callback = Some(meta.value()?.parse()?),
                "multiple_of" => validator
                    .arguments
                    .push(("multiple".into(), meta.value()?.parse()?)),
                "length" | "range" => {
                    let mut keys = BTreeSet::new();
                    meta.parse_nested_meta(|bound| {
                        let key = bound
                            .path
                            .get_ident()
                            .ok_or_else(|| bound.error("expected a bound"))?
                            .to_string();
                        if !(matches!(key.as_str(), "min" | "max")
                            || key == "exact" && name == "length")
                        {
                            return Err(bound.error("unknown input validator bound"));
                        }
                        if !keys.insert(key.clone()) {
                            return Err(bound.error("duplicate input validator bound"));
                        }
                        validator.arguments.push((key, bound.value()?.parse()?));
                        Ok(())
                    })?;
                    if keys.is_empty() || (keys.contains("exact") && keys.len() != 1) {
                        return Err(meta.error("expected min/max bounds or one exact length"));
                    }
                    let min = validator.arguments.iter().find(|(key, _)| key == "min");
                    let max = validator.arguments.iter().find(|(key, _)| key == "max");
                    if let (Some((_, min)), Some((_, max))) = (min, max) {
                        if compare(min, max)? == Ordering::Greater {
                            return Err(meta.error("minimum input bound exceeds maximum"));
                        }
                    }
                }
                _ => return Err(meta.error("unknown input validator")),
            }
            for (_, argument) in &validator.arguments {
                let value = numeric(argument)?;
                if name == "multiple_of" && value == 0.0 {
                    return Err(Error::new(argument.span(), "multiple_of must be nonzero"));
                }
                if name == "length"
                    && !matches!(argument, Expr::Lit(expr) if matches!(expr.lit, Lit::Int(_)))
                {
                    return Err(Error::new(
                        argument.span(),
                        "length bounds must be nonnegative integer literals",
                    ));
                }
            }
            validators.push(validator);
            Ok(())
        })?;
        if validators.len() == before {
            return Err(Error::new(
                attribute.span(),
                "expected at least one input validator",
            ));
        }
    }
    if names.contains("positive") && names.contains("negative") {
        return Err(Error::new(
            validators.last().unwrap().span,
            "positive and negative input validators contradict each other",
        ));
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
    Ok(validators)
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

pub(super) fn numeric(expression: &Expr) -> syn::Result<f64> {
    let (negative, literal) = literal(expression)?;
    let value = match literal {
        Lit::Int(value) => value.base10_parse::<f64>()?,
        Lit::Float(value) => value.base10_parse::<f64>()?,
        _ => {
            return Err(Error::new(
                expression.span(),
                "expected a finite numeric literal",
            ));
        }
    };
    if !value.is_finite() {
        return Err(Error::new(
            expression.span(),
            "expected a finite numeric literal",
        ));
    }
    Ok(if negative { -value } else { value })
}

pub(super) fn check_number(expression: &Expr, name: &str) -> syn::Result<()> {
    let (negative, literal) = literal(expression)?;
    let overflow = || {
        Error::new(
            expression.span(),
            "numeric literal is outside the input field type",
        )
    };
    let suffix = match literal {
        Lit::Int(value) => value.suffix(),
        Lit::Float(value) => value.suffix(),
        _ => return Err(overflow()),
    };
    if !suffix.is_empty() && suffix != name {
        return Err(Error::new(
            expression.span(),
            "numeric literal suffix does not match the input field type",
        ));
    }
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
    let value = value.base10_parse::<u128>()?;
    let unsigned = name.starts_with('u');
    if unsigned && negative {
        return Err(overflow());
    }
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

fn compare(left: &Expr, right: &Expr) -> syn::Result<Ordering> {
    let (ln, ll) = literal(left)?;
    let (rn, rl) = literal(right)?;
    if let (Lit::Int(left), Lit::Int(right)) = (ll, rl) {
        let left = left.base10_parse::<u128>()?;
        let right = right.base10_parse::<u128>()?;
        return Ok(match (ln && left != 0, rn && right != 0) {
            (true, false) => Ordering::Less,
            (false, true) => Ordering::Greater,
            (false, false) => left.cmp(&right),
            (true, true) => right.cmp(&left),
        });
    }
    Ok(numeric(left)?
        .partial_cmp(&numeric(right)?)
        .expect("finite bounds"))
}

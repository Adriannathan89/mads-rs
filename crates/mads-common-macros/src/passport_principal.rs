//! `PassportPrincipal` derive expansion.

use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::{Data, DeriveInput, Error, Field, Fields, Ident, Result, Type, spanned::Spanned};

use crate::path::common_path;

const POLICY_FIELD_TYPE_ERROR: &str =
    "Passport principal policy fields must provide `.iter()` items implementing `AsRef<str>`";

pub(crate) fn expand(input: DeriveInput) -> Result<TokenStream> {
    reject_item_markers(&input)?;

    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(fields) => &fields.named,
            Fields::Unnamed(fields) => {
                return Err(Error::new(
                    fields.span(),
                    "`PassportPrincipal` supports only named-field structs",
                ));
            }
            Fields::Unit => {
                return Err(Error::new(
                    input.ident.span(),
                    "`PassportPrincipal` supports only named-field structs",
                ));
            }
        },
        Data::Enum(data) => {
            return Err(Error::new(
                data.enum_token.span(),
                "`PassportPrincipal` supports only named-field structs",
            ));
        }
        Data::Union(data) => {
            return Err(Error::new(
                data.union_token.span(),
                "`PassportPrincipal` supports only named-field structs",
            ));
        }
    };

    let mut roles = None;
    let mut permissions = None;
    for field in fields {
        let field_roles = marker_count(field.attrs.iter(), "roles");
        let field_permissions = marker_count(field.attrs.iter(), "permissions");
        if field_roles > 1 || field_permissions > 1 {
            return Err(Error::new(
                field.span(),
                "Passport principal policy marker may appear only once per field",
            ));
        }
        if field_roles == 1 && field_permissions == 1 {
            return Err(Error::new(
                field.span(),
                "a Passport principal field cannot be both `roles` and `permissions`",
            ));
        }

        if field_roles == 1 {
            set_unique(&mut roles, field, "roles")?;
            reject_known_invalid_policy_field(&field.ty)?;
        }
        if field_permissions == 1 {
            set_unique(&mut permissions, field, "permissions")?;
            reject_known_invalid_policy_field(&field.ty)?;
        }
    }

    let common = common_path()?;
    let ident = &input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let role_body = membership_body(roles.as_ref());
    let permission_body = membership_body(permissions.as_ref());

    Ok(quote! {
        impl #impl_generics #common::PassportPrincipal for #ident #type_generics #where_clause {
            fn has_role(&self, requested: &str) -> bool { #role_body }
            fn has_permission(&self, requested: &str) -> bool { #permission_body }
        }
    })
}

fn reject_item_markers(input: &DeriveInput) -> Result<()> {
    if let Some(attribute) = input.attrs.iter().find(|attribute| {
        attribute.path().is_ident("roles") || attribute.path().is_ident("permissions")
    }) {
        return Err(Error::new(
            attribute.span(),
            "Passport principal policy markers are valid only on named fields",
        ));
    }
    Ok(())
}

fn marker_count<'a>(attributes: impl Iterator<Item = &'a syn::Attribute>, marker: &str) -> usize {
    attributes
        .filter(|attribute| attribute.path().is_ident(marker))
        .count()
}

fn reject_known_invalid_policy_field(ty: &Type) -> Result<()> {
    if is_scalar_primitive(ty) || is_known_collection_with_primitive_item(ty) {
        return Err(Error::new(ty.span(), POLICY_FIELD_TYPE_ERROR));
    }
    Ok(())
}

fn is_scalar_primitive(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };
    if !matches!(segment.arguments, syn::PathArguments::None) {
        return false;
    }
    matches!(
        segment.ident.to_string().as_str(),
        "bool"
            | "char"
            | "f32"
            | "f64"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}

fn is_known_collection_with_primitive_item(ty: &Type) -> bool {
    let Type::Path(type_path) = ty else {
        return false;
    };
    if type_path.qself.is_some() {
        return false;
    }
    let Some(segment) = type_path.path.segments.last() else {
        return false;
    };
    if !is_known_standard_collection_path(&type_path.path) {
        return false;
    }
    let syn::PathArguments::AngleBracketed(arguments) = &segment.arguments else {
        return false;
    };
    let mut types = arguments.args.iter().filter_map(|argument| match argument {
        syn::GenericArgument::Type(ty) => Some(ty),
        _ => None,
    });
    let Some(item) = types.next() else {
        return false;
    };
    types.next().is_none() && is_scalar_primitive(item)
}

fn is_known_standard_collection_path(path: &syn::Path) -> bool {
    match path
        .segments
        .last()
        .map(|segment| segment.ident.to_string())
    {
        Some(name) if name == "Vec" => {
            path_matches(path, &["std", "vec", "Vec"])
                || path_matches(path, &["alloc", "vec", "Vec"])
        }
        Some(name)
            if matches!(
                name.as_str(),
                "VecDeque" | "LinkedList" | "BinaryHeap" | "BTreeSet"
            ) =>
        {
            path_matches(path, &["std", "collections", &name])
                || path_matches(path, &["alloc", "collections", &name])
        }
        Some(name) if name == "HashSet" => path_matches(path, &["std", "collections", "HashSet"]),
        _ => false,
    }
}

fn path_matches(path: &syn::Path, expected: &[&str]) -> bool {
    path.segments.len() == expected.len()
        && path
            .segments
            .iter()
            .zip(expected)
            .all(|(segment, expected)| segment.ident == *expected)
}

struct PolicyField {
    ident: Ident,
    ty: Type,
}

fn set_unique(slot: &mut Option<PolicyField>, field: &Field, marker: &str) -> Result<()> {
    if slot.is_some() {
        let ident = field.ident.as_ref().expect("named fields have identifiers");
        return Err(Error::new(
            ident.span(),
            format!("duplicate `#[{marker}]` Passport principal field"),
        ));
    }
    *slot = Some(PolicyField {
        ident: field.ident.clone().expect("named fields have identifiers"),
        ty: field.ty.clone(),
    });
    Ok(())
}

fn membership_body(field: Option<&PolicyField>) -> TokenStream {
    field.map_or_else(
        || quote!(false),
        |field| {
            let ident = &field.ident;
            let ty = &field.ty;
            let type_span = ty.span();
            let policy_values = quote_spanned! {type_span=>
                let __mads_policy_values: &#ty = &self.#ident;
            };
            let membership = quote_spanned! {type_span=>
                __mads_contains_passport_policy_string_item(__mads_policy_values.iter(), requested)
            };
            quote! {
                fn __mads_contains_passport_policy_string_item<I>(mut values: I, requested: &str) -> bool
                where
                    I: ::core::iter::Iterator<Item: ::core::convert::AsRef<str>>,
                {
                    values.any(|value| {
                        ::core::convert::AsRef::<str>::as_ref(&value) == requested
                    })
                }

                #policy_values
                #membership
            }
        },
    )
}

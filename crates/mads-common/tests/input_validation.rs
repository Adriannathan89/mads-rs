//! Transport-independent input validation contracts.

#![cfg(feature = "http")]

use mads_common::{
    Input, ValidationErrors, ValidationIssue, ValidationPathSegment, ValidationResult,
};

struct ManualInput {
    password: String,
}

impl Input for ManualInput {
    fn validate(&self) -> ValidationResult {
        if self.password.len() < 20 {
            Err(ValidationErrors::from_issue(
                ValidationIssue::custom("weak_password", "password is too short")
                    .at_field("users")
                    .at_index(0)
                    .at_field("password"),
            ))
        } else {
            Ok(())
        }
    }
}

#[test]
fn public_model_preserves_relative_paths_and_issue_order_without_values() {
    let input = ManualInput {
        password: "secret-sentinel".into(),
    };
    let mut errors = input.validate().unwrap_err();
    errors.push(ValidationIssue::custom("second", "second issue"));
    errors.extend(ValidationErrors::from_issue(ValidationIssue::custom(
        "third",
        "third issue",
    )));
    assert_eq!(
        errors.issues()[0].path(),
        [
            ValidationPathSegment::Field("users".into()),
            ValidationPathSegment::Index(0),
            ValidationPathSegment::Field("password".into()),
        ]
    );
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.code())
            .collect::<Vec<_>>(),
        ["weak_password", "second", "third"]
    );
    assert_eq!(errors.issues()[0].message(), "password is too short");
    assert!(errors.issues()[1].path().is_empty());
    assert!(!format!("{errors:?} {errors}").contains("secret-sentinel"));
    assert!(std::error::Error::source(&errors).is_none());
    let issues = errors.into_issues();
    assert_eq!(issues.len(), 3);
    assert_eq!(issues[2].code(), "third");
}

#[test]
fn builtin_validator_matrix_checks_numeric_widths_and_decimal_multiples() {
    use mads_common::__private::input_validation::Number;
    macro_rules! unsigned {
        ($($ty:ty),+ $(,)?) => {$(
            assert!(Number::multiple_of(6 as $ty, 2));
            assert!(!Number::multiple_of(7 as $ty, 2));
            assert!(!Number::multiple_of(0 as $ty, 0));
        )+};
    }
    unsigned!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
    macro_rules! signed {
        ($($ty:ty),+ $(,)?) => {$(
            assert!(Number::multiple_of(<$ty>::MIN, -1));
            assert!(Number::multiple_of(-6 as $ty, -2));
            assert!(!Number::multiple_of(-7 as $ty, -2));
        )+};
    }
    signed!(i8, i16, i32, i64, i128, isize);
    assert!(Number::multiple_of(0.3_f64, 0.1));
    assert!(Number::multiple_of(0.3_f32, 0.1));
    assert!(!Number::multiple_of(0.31_f64, 0.1));
    assert!(!Number::multiple_of(f64::NAN, 0.1));
    assert!(!Number::finite(f32::INFINITY));
    assert!(!Number::finite(f64::NEG_INFINITY));
}

#[test]
fn builtin_validator_matrix_counts_codepoints_and_traverses_ordered_collections() {
    use mads_common::__private::input_validation::{Length, email};
    assert_eq!(Length::length("é🦀"), 2);
    assert_eq!(Length::length(&String::from("é🦀")), 2);
    assert_eq!(Length::length(&vec![1, 2, 3]), 3);
    assert_eq!(Length::length(&[1, 2]), 2);
    assert!(email("person@example.com"));
    assert!(!email("é@example.com"));
    assert!(!email("a..b@example.com"));
    let mut map = std::collections::HashMap::new();
    map.insert(
        "z".to_owned(),
        ManualInput {
            password: "short".into(),
        },
    );
    map.insert(
        "a".to_owned(),
        ManualInput {
            password: "short".into(),
        },
    );
    assert_eq!(Length::length(&map), 2);
    let errors = map.validate().unwrap_err();
    assert_eq!(
        errors.issues()[0].path()[0],
        ValidationPathSegment::Field("a".into())
    );
    assert_eq!(
        errors.issues()[1].path()[0],
        ValidationPathSegment::Field("z".into())
    );
    let sequence = vec![
        Some(ManualInput {
            password: "short".into(),
        }),
        None,
    ];
    let errors = sequence.validate().unwrap_err();
    assert_eq!(errors.issues().len(), 1);
    assert_eq!(
        errors.issues()[0].path()[0],
        ValidationPathSegment::Index(0)
    );
    assert_eq!(
        errors.issues()[0].path()[1],
        ValidationPathSegment::Field("users".into())
    );
}

#[test]
fn builtin_validator_matrix_uses_frozen_messages_without_rejected_values() {
    use mads_common::__private::input_validation::{IssueKind, issue};
    for (kind, code, message) in [
        (IssueKind::Required, "required", "required value is missing"),
        (
            IssueKind::InvalidType,
            "invalid_type",
            "input has an invalid type",
        ),
        (
            IssueKind::InvalidSyntax,
            "invalid_syntax",
            "input syntax is invalid",
        ),
        (IssueKind::Email, "invalid_format", "invalid email address"),
        (IssueKind::TooSmall, "too_small", "value is too small"),
        (IssueKind::TooBig, "too_big", "value is too big"),
        (
            IssueKind::NotMultipleOf,
            "not_multiple_of",
            "value is not a multiple of the required number",
        ),
    ] {
        let failure = issue(kind);
        assert_eq!(failure.code(), code);
        assert_eq!(failure.message(), message);
        assert!(failure.path().is_empty());
    }
}

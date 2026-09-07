//! Transport-independent input validation contracts.

#![cfg(feature = "http")]

use mads_common::{
    Input, ValidationErrors, ValidationIssue, ValidationPathSegment, ValidationResult,
};

fn issue_summary(errors: ValidationErrors) -> Vec<(Vec<ValidationPathSegment>, String)> {
    errors
        .into_issues()
        .into_iter()
        .map(|issue| (issue.path().to_vec(), issue.code().to_owned()))
        .collect()
}

#[derive(serde::Deserialize, Input)]
struct ProfileInput {
    #[validate(length(min = 2, max = 4))]
    name: String,
}

#[derive(serde::Deserialize, Input)]
#[serde(rename_all = "camelCase")]
struct CreateUserInput {
    #[validate(email, length(max = 10))]
    email_address: String,
    #[validate(required, nested)]
    profile: Option<ProfileInput>,
    #[validate(range(min = 1, max = 10), multiple_of = 2)]
    count: i64,
    #[validate(positive, multiple_of = 0.1)]
    ratio: f64,
}

#[test]
fn derive_matrix_collects_ordered_named_option_and_numeric_issues() {
    use ValidationPathSegment::Field;
    let input = CreateUserInput {
        email_address: "invalid-address".into(),
        profile: None,
        count: -3,
        ratio: f64::NAN,
    };
    assert_eq!(
        issue_summary(input.validate().unwrap_err()),
        vec![
            (vec![Field("emailAddress".into())], "invalid_format".into()),
            (vec![Field("emailAddress".into())], "too_big".into()),
            (vec![Field("profile".into())], "required".into()),
            (vec![Field("count".into())], "too_small".into()),
            (vec![Field("count".into())], "not_multiple_of".into()),
            (vec![Field("ratio".into())], "invalid_type".into()),
        ]
    );
    let input = CreateUserInput {
        email_address: "a@b.co".into(),
        profile: Some(ProfileInput {
            name: "é🦀".into()
        }),
        count: 4,
        ratio: 0.3,
    };
    assert!(input.validate().is_ok());
}

#[test]
fn derive_matrix_serde_paths_and_all_data_forms() {
    use ValidationPathSegment::{Field, Index};
    #[derive(serde::Deserialize, Input)]
    struct Flattened {
        #[serde(flatten)]
        #[validate(nested)]
        profile: ProfileInput,
        #[serde(
            rename(deserialize = "external", serialize = "output"),
            alias = "legacy",
            default
        )]
        #[validate(nonempty)]
        value: String,
    }
    let value: Flattened = serde_json::from_str(r#"{"name":"x","legacy":""}"#).unwrap();
    assert_eq!(
        issue_summary(value.validate().unwrap_err()),
        vec![
            (vec![Field("name".into())], "too_small".into()),
            (vec![Field("external".into())], "too_small".into()),
        ]
    );
    #[derive(Input)]
    struct Unit;
    #[derive(Input)]
    struct Tuple(#[validate(nonempty)] String, f32);
    assert!(Unit.validate().is_ok());
    assert_eq!(
        issue_summary(Tuple(String::new(), f32::INFINITY).validate().unwrap_err()),
        vec![
            (vec![Index(0)], "too_small".into()),
            (vec![Index(1)], "invalid_type".into()),
        ]
    );
    #[derive(serde::Deserialize, Input)]
    #[serde(rename_all = "snake_case")]
    enum External {
        Named {
            #[validate(nonempty)]
            value: String,
        },
        Tuple(#[validate(nonempty)] String),
        Unit,
    }
    assert!(External::Unit.validate().is_ok());
    assert_eq!(
        issue_summary(
            External::Named {
                value: String::new()
            }
            .validate()
            .unwrap_err()
        ),
        vec![(
            vec![Field("named".into()), Field("value".into())],
            "too_small".into()
        ),]
    );
    assert_eq!(
        issue_summary(External::Tuple(String::new()).validate().unwrap_err()),
        vec![(vec![Field("tuple".into()), Index(0)], "too_small".into()),]
    );
    #[derive(serde::Deserialize, Input)]
    #[serde(tag = "kind", content = "data")]
    enum Adjacent {
        Named {
            #[validate(nonempty)]
            value: String,
        },
    }
    assert_eq!(
        issue_summary(
            Adjacent::Named {
                value: String::new()
            }
            .validate()
            .unwrap_err()
        ),
        vec![(
            vec![Field("data".into()), Field("value".into())],
            "too_small".into()
        ),]
    );
    #[derive(serde::Deserialize, Input)]
    #[serde(tag = "kind")]
    enum Internal {
        Named {
            #[validate(nonempty)]
            value: String,
        },
    }
    assert_eq!(
        issue_summary(
            Internal::Named {
                value: String::new()
            }
            .validate()
            .unwrap_err()
        ),
        vec![(vec![Field("value".into())], "too_small".into()),]
    );
    #[derive(serde::Deserialize, Input)]
    #[serde(untagged)]
    enum Untagged {
        Named {
            #[validate(nonempty)]
            value: String,
        },
    }
    assert_eq!(
        issue_summary(
            Untagged::Named {
                value: String::new()
            }
            .validate()
            .unwrap_err()
        ),
        vec![(vec![Field("value".into())], "too_small".into()),]
    );
}

#[test]
fn derive_matrix_serde_variant_renames_defaults_and_whole_enum_callback() {
    use ValidationPathSegment::Field;
    fn whole(_: &Message) -> ValidationResult {
        Err(ValidationErrors::from_issue(ValidationIssue::custom(
            "whole",
            "whole enum issue",
        )))
    }
    #[derive(serde::Deserialize, Input)]
    #[serde(rename_all_fields = "camelCase")]
    #[validate(custom = whole)]
    enum Message {
        #[serde(rename(deserialize = "request", serialize = "response"))]
        Named {
            #[serde(default, alias = "legacy")]
            #[validate(nonempty)]
            first_name: String,
        },
        #[serde(rename_all = "SCREAMING_SNAKE_CASE")]
        Other {
            #[validate(nonempty)]
            last_name: String,
        },
    }
    for json in [
        r#"{"request":{}}"#,
        r#"{"request":{"legacy":""}}"#,
        r#"{"request":{"firstName":"","ignored":true}}"#,
    ] {
        let input: Message = serde_json::from_str(json).unwrap();
        assert_eq!(
            issue_summary(input.validate().unwrap_err()),
            vec![
                (
                    vec![Field("request".into()), Field("firstName".into())],
                    "too_small".into()
                ),
                (vec![], "whole".into()),
            ]
        );
    }
    let input: Message = serde_json::from_str(r#"{"Other":{"LAST_NAME":""}}"#).unwrap();
    assert_eq!(
        input.validate().unwrap_err().issues()[0].path(),
        [Field("Other".into()), Field("LAST_NAME".into())]
    );
    #[derive(serde::Deserialize, Input)]
    #[serde(deny_unknown_fields)]
    struct Strict {
        #[validate(nonempty)]
        name: String,
    }
    assert!(serde_json::from_str::<Strict>(r#"{"name":"ok","unknown":true}"#).is_err());
}

#[test]
fn custom_and_generic_callbacks_preserve_paths_and_run_whole_value_last() {
    use ValidationPathSegment::{Field, Index};
    fn callback(_: &str) -> ValidationResult {
        let mut errors = ValidationErrors::from_issue(ValidationIssue::custom("first", "first"));
        errors.push(ValidationIssue::custom("second", "second").at_field("detail"));
        Err(errors)
    }
    fn whole<T, const N: usize>(_: &Generic<'_, T, N>) -> ValidationResult {
        Err(ValidationErrors::from_issue(ValidationIssue::custom(
            "whole", "whole",
        )))
    }
    #[derive(Input)]
    #[validate(custom = whole)]
    struct Generic<'a, T, const N: usize> {
        #[validate(custom = callback)]
        borrowed: &'a str,
        #[validate(nested)]
        values: [T; N],
        #[validate(nested, length(exact = 2))]
        tuple: (ProfileInput, ProfileInput),
    }
    let input = Generic {
        borrowed: "anything",
        values: [ProfileInput { name: "x".into() }],
        tuple: (
            ProfileInput { name: "ab".into() },
            ProfileInput { name: "x".into() },
        ),
    };
    assert_eq!(
        issue_summary(input.validate().unwrap_err()),
        vec![
            (vec![Field("borrowed".into())], "first".into()),
            (
                vec![Field("borrowed".into()), Field("detail".into())],
                "second".into()
            ),
            (
                vec![Field("values".into()), Index(0), Field("name".into())],
                "too_small".into()
            ),
            (
                vec![Field("tuple".into()), Index(1), Field("name".into())],
                "too_small".into()
            ),
            (vec![], "whole".into()),
        ]
    );
}

#[test]
fn derive_matrix_recurses_through_composed_collections() {
    use ValidationPathSegment::{Field, Index};
    use std::collections::{BTreeMap, HashMap};
    #[derive(Input)]
    struct Collections<T> {
        #[validate(nested, nonempty)]
        values: Vec<Option<(T, [T; 1])>>,
        #[validate(nested, length(min = 1))]
        hash: HashMap<String, (T,)>,
        #[validate(nested)]
        tree: BTreeMap<String, T>,
    }
    let bad = || ProfileInput {
        name: String::new(),
    };
    let input = Collections {
        values: vec![None, Some((bad(), [bad()]))],
        hash: HashMap::from([("z".into(), (bad(),)), ("a".into(), (bad(),))]),
        tree: BTreeMap::from([("b".into(), bad())]),
    };
    let errors = input.validate().unwrap_err();
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.path().to_vec())
            .collect::<Vec<_>>(),
        vec![
            vec![
                Field("values".into()),
                Index(1),
                Index(0),
                Field("name".into())
            ],
            vec![
                Field("values".into()),
                Index(1),
                Index(1),
                Index(0),
                Field("name".into())
            ],
            vec![
                Field("hash".into()),
                Field("a".into()),
                Index(0),
                Field("name".into())
            ],
            vec![
                Field("hash".into()),
                Field("z".into()),
                Index(0),
                Field("name".into())
            ],
            vec![
                Field("tree".into()),
                Field("b".into()),
                Field("name".into())
            ],
        ]
    );
}

#[test]
fn derive_matrix_nested_primitives_reject_nonfinite_elements() {
    use ValidationPathSegment::{Field, Index};
    #[derive(Input)]
    struct PrimitiveCollections {
        #[validate(nested)]
        values: Vec<f64>,
        #[validate(nested)]
        pair: (String, [Option<f32>; 2]),
    }
    let input = PrimitiveCollections {
        values: vec![0.3, f64::NAN, f64::INFINITY],
        pair: (String::new(), [None, Some(f32::NEG_INFINITY)]),
    };
    assert_eq!(
        issue_summary(input.validate().unwrap_err()),
        vec![
            (
                vec![Field("values".into()), Index(1)],
                "invalid_type".into()
            ),
            (
                vec![Field("values".into()), Index(2)],
                "invalid_type".into()
            ),
            (
                vec![Field("pair".into()), Index(1), Index(1)],
                "invalid_type".into()
            ),
        ]
    );
    assert!(vec![1_u128, u128::MAX].validate().is_ok());
    assert!(vec!["", "anything"].validate().is_ok());
    assert!(false.validate().is_ok());
    assert!('🦀'.validate().is_ok());
    #[derive(Input)]
    struct NestedFloat {
        #[validate(nested)]
        value: f64,
    }
    assert_eq!(
        NestedFloat { value: f64::NAN }
            .validate()
            .unwrap_err()
            .issues()
            .len(),
        1
    );
}

#[test]
fn derive_matrix_frozen_email_policy_and_unicode_length() {
    #[derive(Input)]
    struct Email<'a> {
        #[validate(email)]
        value: &'a str,
    }
    for value in [
        "a@b.co",
        "O'Neil@example.com",
        "a+b@example.com",
        "a_b@example.com",
        "a@sub.example.com",
        "a@host-.com",
    ] {
        assert!(Email { value }.validate().is_ok(), "{value}");
    }
    for value in [
        "",
        "a",
        "a@b",
        ".a@example.com",
        "a.@example.com",
        "a..b@example.com",
        "a'@example.com",
        "a@-host.com",
        "a@host.c",
        "a@host.12",
        "a@host..com",
        "a@host.com\n",
        "é@example.com",
        "a@é.com",
        "a b@example.com",
        "a@example.com ",
        "\"a\"@example.com",
        "a@127.0.0.1",
    ] {
        let errors = Email { value }.validate().unwrap_err();
        assert_eq!(errors.issues()[0].code(), "invalid_format");
        assert_eq!(errors.issues()[0].message(), "invalid email address");
    }
    #[derive(Input)]
    struct Unicode<'a> {
        #[validate(length(exact = 2))]
        value: &'a str,
    }
    assert!(Unicode { value: "é🦀" }.validate().is_ok());
    assert_eq!(
        Unicode { value: "é" }.validate().unwrap_err().issues()[0].code(),
        "too_small"
    );
    assert_eq!(
        Unicode { value: "é🦀a" }.validate().unwrap_err().issues()[0].code(),
        "too_big"
    );
}

#[test]
fn derive_matrix_numeric_boundaries_and_primitive_fields() {
    macro_rules! integer {
        ($($ty:ty),+ $(,)?) => {$( {
            #[derive(Input)]
            struct Numeric {
                #[validate(range(min = 2, max = 8), positive, multiple_of = 2)]
                value: $ty,
            }
            assert!(Numeric { value: 2 }.validate().is_ok());
            assert!(Numeric { value: 8 }.validate().is_ok());
            assert_eq!(Numeric { value: 9 }.validate().unwrap_err().issues().iter().map(|issue| issue.code()).collect::<Vec<_>>(), ["too_big", "not_multiple_of"]);
            assert_eq!(Numeric { value: 0 }.validate().unwrap_err().issues().iter().map(|issue| issue.code()).collect::<Vec<_>>(), ["too_small", "too_small"]);
        } )+};
    }
    integer!(
        u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
    );
    macro_rules! signed {
        ($($ty:ty),+ $(,)?) => {$( {
            #[derive(Input)]
            struct Negative { #[validate(negative, multiple_of = -1)] value: $ty }
            assert!(Negative { value: <$ty>::MIN }.validate().is_ok());
            assert_eq!(Negative { value: 0 }.validate().unwrap_err().issues()[0].code(), "too_big");
        } )+};
    }
    signed!(i8, i16, i32, i64, i128, isize);
    macro_rules! floats {
        ($($ty:ty),+ $(,)?) => {$( {
            #[derive(Input)]
            struct Float { #[validate(range(min = -1, max = 1), multiple_of = 0.1)] value: $ty }
            for value in [-1.0, 0.3, 1.0] { assert!(Float { value }.validate().is_ok()); }
            for value in [<$ty>::NAN, <$ty>::INFINITY, <$ty>::NEG_INFINITY] {
                let errors = Float { value }.validate().unwrap_err();
                assert_eq!(errors.issues().len(), 1);
                assert_eq!(errors.issues()[0].code(), "invalid_type");
            }
        } )+};
    }
    floats!(f32, f64);
    #[derive(Input)]
    struct Primitive {
        boolean: bool,
        character: char,
        float: Option<f32>,
    }
    let input = Primitive {
        boolean: false,
        character: '🦀',
        float: None,
    };
    assert!(input.validate().is_ok());
    assert!(!input.boolean);
    assert_eq!(input.character, '🦀');
    assert!(
        Primitive {
            float: Some(f32::NAN),
            ..input
        }
        .validate()
        .is_err()
    );
}

#[test]
fn custom_and_generic_optional_callbacks_and_nonfinite_order() {
    fn custom(_: &f64) -> ValidationResult {
        Err(ValidationErrors::from_issue(ValidationIssue::custom(
            "custom",
            "custom failure",
        )))
    }
    #[derive(Input)]
    struct Float {
        #[validate(custom = custom, positive)]
        value: Option<f64>,
    }
    assert!(Float { value: None }.validate().is_ok());
    let errors = Float {
        value: Some(f64::NAN),
    }
    .validate()
    .unwrap_err();
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.code())
            .collect::<Vec<_>>(),
        ["invalid_type", "custom"]
    );
    let errors = Float { value: Some(-1.0) }.validate().unwrap_err();
    assert_eq!(
        errors
            .issues()
            .iter()
            .map(|issue| issue.code())
            .collect::<Vec<_>>(),
        ["custom", "too_small"]
    );
}

#[test]
fn custom_and_generic_accepts_arbitrary_borrowed_field_types() {
    fn custom(value: &f64) -> ValidationResult {
        assert_eq!(*value, 0.3);
        Ok(())
    }
    #[derive(Input)]
    struct Borrowed<'a> {
        #[validate(custom = custom)]
        value: &'a f64,
    }
    assert!(Borrowed { value: &0.3 }.validate().is_ok());
}

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

//! Intrinsic validation for nested primitive values.

use super::{
    Input, ValidationErrors, ValidationResult,
    support::{IssueKind, issue},
};

macro_rules! unconstrained {
    ($($ty:ty),+ $(,)?) => {$(
        impl Input for $ty {
            fn validate(&self) -> ValidationResult { Ok(()) }
        }
    )+};
}
unconstrained!(
    bool, char, str, String, u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

macro_rules! floats {
    ($($ty:ty),+ $(,)?) => {$(
        impl Input for $ty {
            fn validate(&self) -> ValidationResult {
                if self.is_finite() { Ok(()) }
                else { Err(ValidationErrors::from_issue(issue(IssueKind::InvalidType))) }
            }
        }
    )+};
}
floats!(f32, f64);

impl<T: Input + ?Sized> Input for &T {
    fn validate(&self) -> ValidationResult {
        T::validate(self)
    }
}

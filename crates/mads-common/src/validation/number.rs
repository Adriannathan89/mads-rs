//! Finite and multiple checks that preserve exact integer arithmetic.

/// Numeric operations used by generated built-in validators.
pub trait Number: Copy + PartialOrd {
    /// Returns whether this value is finite.
    fn finite(self) -> bool;
    /// Checks a nonzero multiple, tolerating decimal representation noise for floats.
    fn multiple_of(self, divisor: Self) -> bool;
}

macro_rules! integers {
    ($($ty:ty),+ $(,)?) => {$(
        impl Number for $ty {
            fn finite(self) -> bool { true }
            fn multiple_of(self, divisor: Self) -> bool {
                // checked_rem also avoids signed MIN / -1 overflow: its
                // mathematical remainder is zero for that sole nonzero case.
                divisor != 0 && matches!(self.checked_rem(divisor), Some(0) | None)
            }
        }
    )+};
}
integers!(
    u8, u16, u32, u64, u128, usize, i8, i16, i32, i64, i128, isize
);

impl Number for f32 {
    fn finite(self) -> bool {
        self.is_finite()
    }
    fn multiple_of(self, divisor: Self) -> bool {
        mads_core::__private::configuration_validation::multiple(
            self as f64,
            divisor as f64,
            Self::EPSILON as f64,
        )
    }
}

impl Number for f64 {
    fn finite(self) -> bool {
        self.is_finite()
    }
    fn multiple_of(self, divisor: Self) -> bool {
        mads_core::__private::configuration_validation::multiple(self, divisor, Self::EPSILON)
    }
}

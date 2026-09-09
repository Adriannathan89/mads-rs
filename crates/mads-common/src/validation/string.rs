//! String syntax and Unicode-aware length helpers.

use std::collections::{BTreeMap, HashMap};

/// Measures values according to the input validation length contract.
pub trait Length {
    /// Returns code-point length for strings or entry count for collections.
    fn length(&self) -> usize;
}

impl Length for str {
    fn length(&self) -> usize {
        self.chars().count()
    }
}

impl Length for String {
    fn length(&self) -> usize {
        self.chars().count()
    }
}

impl<T: Length + ?Sized> Length for &T {
    fn length(&self) -> usize {
        (**self).length()
    }
}

impl<T> Length for Vec<T> {
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T, const N: usize> Length for [T; N] {
    fn length(&self) -> usize {
        N
    }
}

impl<T, S> Length for HashMap<String, T, S> {
    fn length(&self) -> usize {
        self.len()
    }
}

impl<T> Length for BTreeMap<String, T> {
    fn length(&self) -> usize {
        self.len()
    }
}

/// Checks the same practical email policy used by typed configuration.
pub fn email(value: &str) -> bool {
    mads_core::__private::configuration_validation::email(value)
}

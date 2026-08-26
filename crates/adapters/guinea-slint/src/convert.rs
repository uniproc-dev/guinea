//! Rust values as Slint ones.
//!
//! Every binding ends in a setter, and every setter wants a Slint type: a
//! `String` has to become a `SharedString`, a `Vec` a model. Written out at
//! each call that is three lines of ceremony around one field, repeated per
//! property - so it is written once, here.

use slint::{ModelRc, SharedString, VecModel};

/// What a value looks like on the other side of a property.
pub trait ToSlint {
    type Slint;

    fn to_slint(&self) -> Self::Slint;
}

macro_rules! itself {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ToSlint for $ty {
                type Slint = $ty;

                fn to_slint(&self) -> Self::Slint {
                    *self
                }
            }
        )*
    };
}

itself!(bool, i32, f32);

macro_rules! as_int {
    ($($ty:ty),* $(,)?) => {
        $(
            /// Slint's `int` is an `i32`, and a count that does not fit in one
            /// is not a number a UI can show anyway.
            impl ToSlint for $ty {
                type Slint = i32;

                fn to_slint(&self) -> Self::Slint {
                    *self as i32
                }
            }
        )*
    };
}

as_int!(i8, i16, i64, isize, u8, u16, u32, u64, usize);

impl ToSlint for f64 {
    type Slint = f32;

    fn to_slint(&self) -> Self::Slint {
        *self as f32
    }
}

impl ToSlint for String {
    type Slint = SharedString;

    fn to_slint(&self) -> Self::Slint {
        SharedString::from(self.as_str())
    }
}

impl ToSlint for str {
    type Slint = SharedString;

    fn to_slint(&self) -> Self::Slint {
        SharedString::from(self)
    }
}

impl<T: ToSlint + ?Sized> ToSlint for &T {
    type Slint = T::Slint;

    fn to_slint(&self) -> Self::Slint {
        (*self).to_slint()
    }
}

/// `None` becomes whatever the property's type calls empty - an empty string,
/// a zero, an empty model.
impl<T: ToSlint> ToSlint for Option<T>
where
    T::Slint: Default,
{
    type Slint = T::Slint;

    fn to_slint(&self) -> Self::Slint {
        match self {
            Some(value) => value.to_slint(),
            None => T::Slint::default(),
        }
    }
}

impl<T: ToSlint> ToSlint for Vec<T>
where
    T::Slint: Clone + 'static,
{
    type Slint = ModelRc<T::Slint>;

    fn to_slint(&self) -> Self::Slint {
        self.as_slice().to_slint()
    }
}

impl<T: ToSlint> ToSlint for [T]
where
    T::Slint: Clone + 'static,
{
    type Slint = ModelRc<T::Slint>;

    fn to_slint(&self) -> Self::Slint {
        let rows: Vec<T::Slint> = self.iter().map(ToSlint::to_slint).collect();
        ModelRc::new(VecModel::from(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::ToSlint;
    use slint::Model;

    #[test]
    fn a_list_of_strings_becomes_a_model_of_shared_strings() {
        let items = vec!["systemd".to_string(), "sshd".to_string()];
        let model = items.to_slint();

        assert_eq!(model.row_count(), 2);
        assert_eq!(model.row_data(0).unwrap(), "systemd");
    }

    #[test]
    fn a_count_becomes_an_int() {
        assert_eq!(7usize.to_slint(), 7i32);
    }

    #[test]
    fn nothing_becomes_the_empty_value() {
        let missing: Option<String> = None;
        assert_eq!(missing.to_slint(), "");
    }
}

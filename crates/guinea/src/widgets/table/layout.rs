use std::cell::Cell;
use std::collections::HashMap;
use std::hash::Hash;
use std::rc::Rc;

/// A column width the table reads and writes. `Width::bound` lets the caller
/// keep it wherever it likes - a settings cell, for instance - instead of the
/// table owning it.
#[derive(Clone)]
pub struct Width {
    get: Rc<dyn Fn() -> u64>,
    set: Rc<dyn Fn(u64)>,
}

impl Width {
    pub fn fixed(initial: u64) -> Self {
        let cell = Rc::new(Cell::new(initial));
        let read = cell.clone();
        Self {
            get: Rc::new(move || read.get()),
            set: Rc::new(move |v| cell.set(v)),
        }
    }

    pub fn bound(get: impl Fn() -> u64 + 'static, set: impl Fn(u64) + 'static) -> Self {
        Self {
            get: Rc::new(get),
            set: Rc::new(set),
        }
    }

    pub fn get(&self) -> u64 {
        (self.get)()
    }

    pub fn set(&self, value: u64) {
        (self.set)(value)
    }
}

pub trait IntoWidth {
    fn into_width(self) -> Width;
}

impl IntoWidth for u64 {
    fn into_width(self) -> Width {
        Width::fixed(self)
    }
}

impl IntoWidth for Width {
    fn into_width(self) -> Width {
        self
    }
}

pub struct TableLayout<ID> {
    widths: HashMap<ID, Width>,
}

impl<ID> Default for TableLayout<ID>
where
    ID: Hash + Eq + Clone,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<ID> TableLayout<ID>
where
    ID: Hash + Eq + Clone,
{
    pub fn new() -> Self {
        Self { widths: HashMap::new() }
    }

    pub fn add_column(&mut self, id: ID, initial_width: impl IntoWidth) -> Width {
        self.widths
            .entry(id)
            .or_insert_with(|| initial_width.into_width())
            .clone()
    }

    pub fn width(&self, id: &ID) -> Option<Width> {
        self.widths.get(id).cloned()
    }

    pub fn get_width(&self, id: &ID) -> u64 {
        self.widths.get(id).map(|s| s.get()).unwrap_or(0)
    }

    pub fn set_width(&self, id: &ID, new_width: u64) {
        if let Some(sig) = self.widths.get(id) {
            sig.set(new_width);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_column_is_idempotent_and_reuses_the_same_width() {
        let mut layout = TableLayout::<String>::new();
        let a = layout.add_column("name".into(), 200);
        let b = layout.add_column("name".into(), 999); // second call: initial_width ignored

        a.set(111);
        assert_eq!(b.get(), 111, "a/b are clones of the same underlying width");
        assert_eq!(layout.get_width(&"name".to_string()), 111);
    }

    #[test]
    fn set_width_is_visible_through_the_returned_width() {
        let mut layout = TableLayout::<String>::new();
        let width = layout.add_column("pid".into(), 80);

        layout.set_width(&"pid".to_string(), 120);

        assert_eq!(width.get(), 120);
        assert_eq!(layout.get_width(&"pid".to_string()), 120);
    }

    #[test]
    fn unknown_column_width_defaults_to_zero() {
        let layout = TableLayout::<String>::new();
        assert_eq!(layout.get_width(&"missing".to_string()), 0);
    }

    #[test]
    fn add_column_accepts_a_caller_owned_width_and_reuses_it_as_is() {
        let mut layout = TableLayout::<String>::new();
        let cell = Rc::new(Cell::new(42u64));

        let read = cell.clone();
        let write = cell.clone();
        let returned = layout.add_column(
            "shared".into(),
            Width::bound(move || read.get(), move |v| write.set(v)),
        );

        cell.set(7);
        assert_eq!(returned.get(), 7, "the caller's own storage is used directly, not copied");

        returned.set(9);
        assert_eq!(cell.get(), 9, "writes go back to the caller's storage");
    }
}

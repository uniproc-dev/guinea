use guinea_core::signal::Signal;
use std::collections::HashMap;
use std::hash::Hash;

pub trait IntoWidth {
    fn into_width(self) -> Signal<u64>;
}

impl IntoWidth for u64 {
    fn into_width(self) -> Signal<u64> {
        Signal::new(self)
    }
}

impl IntoWidth for Signal<u64> {
    fn into_width(self) -> Signal<u64> {
        self
    }
}

pub struct TableLayout<ID> {
    widths: HashMap<ID, Signal<u64>>,
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

    pub fn add_column(&mut self, id: ID, initial_width: impl IntoWidth) -> Signal<u64> {
        self.widths
            .entry(id)
            .or_insert_with(|| initial_width.into_width())
            .clone()
    }

    pub fn width(&self, id: &ID) -> Option<Signal<u64>> {
        self.widths.get(id).cloned()
    }

    pub fn get_width(&self, id: &ID) -> u64 {
        self.widths.get(id).map(|s| s.get()).unwrap_or(0)
    }

    pub fn set_width(&self, id: &ID, new_width: u64) {
        if let Some(sig) = self.widths.get(id) {
            sig.set(new_width, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_column_is_idempotent_and_reuses_the_same_signal() {
        let mut layout = TableLayout::<String>::new();
        let a = layout.add_column("name".into(), 200);
        let b = layout.add_column("name".into(), 999); // second call: initial_width ignored

        a.set(111, None);
        assert_eq!(b.get(), 111, "a/b are clones of the same underlying signal");
        assert_eq!(layout.get_width(&"name".to_string()), 111);
    }

    #[test]
    fn set_width_is_visible_through_the_returned_signal() {
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
    fn add_column_accepts_an_existing_signal_and_reuses_it_as_is() {
        let mut layout = TableLayout::<String>::new();
        let shared = Signal::new(42u64);

        let returned = layout.add_column("shared".into(), shared.clone());

        shared.set(7, None);
        assert_eq!(returned.get(), 7, "the caller's own Signal is used directly, not copied");
    }
}

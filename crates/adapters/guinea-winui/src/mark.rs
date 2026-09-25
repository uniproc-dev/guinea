use guinea_core::mark::Mark;
use windows_reactor::{AutomationExt, LayoutControl};

/// Puts a [`Mark`] on an element, as its `AutomationId`.
pub trait MarkExt: AutomationExt {
    fn mark(self, mark: impl Mark) -> Self {
        self.automation_id(mark.name())
    }
}

impl<T: LayoutControl> MarkExt for T {}

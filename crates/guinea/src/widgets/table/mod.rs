mod flow;
mod layout;

pub use flow::{SortState, TableDataBuilder, TableFlowState, TableNode};
pub use layout::{IntoWidth, TableLayout};

use guinea_core::signal::Signal;
use std::rc::Rc;
use windows_reactor::{hstack, Element, ElementExt, RenderCx, SetState};

use crate::widgets::resize::resize_handle;

const MIN_COLUMN_WIDTH: f64 = 24.0;

pub struct ColumnSpec<T> {
    pub id: &'static str,
    pub header: String,
    pub initial_width: Signal<u64>,
    pub sortable: bool,
    pub cell: Rc<dyn Fn(&T) -> Element>,
}

impl<T> ColumnSpec<T> {
    pub fn new(id: &'static str, header: impl Into<String>, initial_width: impl IntoWidth, cell: impl Fn(&T) -> Element + 'static) -> Self {
        Self { id, header: header.into(), initial_width: initial_width.into_width(), sortable: false, cell: Rc::new(cell) }
    }

    /// Makes the header clickable and shows the sort indicator when `table`
    /// receives the matching `SortState`. The sort id is the column `id`.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }
}

struct ResolvedColumn<T> {
    id: &'static str,
    header: String,
    width: Signal<u64>,
    sortable: bool,
    cell: Rc<dyn Fn(&T) -> Element>,
}

pub fn table<T: 'static>(
    cx: &mut RenderCx,
    rows: Vec<T>,
    columns: Vec<ColumnSpec<T>>,
    key: impl Fn(&T) -> String + 'static,
    sort: Option<(SortState<String>, SetState<String>)>,
) -> Element {
    let layout_ref = cx.use_ref(TableLayout::<&'static str>::new());
    let columns: Rc<Vec<ResolvedColumn<T>>> = {
        let mut layout = layout_ref.borrow_mut();
        Rc::new(
            columns
                .into_iter()
                .map(|spec| ResolvedColumn {
                    id: spec.id,
                    width: layout.add_column(spec.id, spec.initial_width),
                    header: spec.header,
                    sortable: spec.sortable,
                    cell: spec.cell,
                })
                .collect(),
        )
    };

    let (_, request_rerender) = cx.use_state(());

    let (sort_state, on_sort) = match sort {
        Some((state, cb)) => (Some(state), Some(cb)),
        None => (None, None),
    };

    let last_index = columns.len().saturating_sub(1);

    let mut header_cells: Vec<Element> = Vec::with_capacity(columns.len() * 2);
    for (i, c) in columns.iter().enumerate() {
        header_cells.push(header_cell(c, sort_state.as_ref(), on_sort.as_ref()));
        if i != last_index {
            header_cells.push(column_resize_handle(cx, c.width.clone(), request_rerender.clone()));
        }
    }
    let header = hstack(header_cells);

    let columns_for_rows = columns.clone();
    let body = windows_reactor::list_view(rows, move |row: &T, _idx: usize| row_view(row, &columns_for_rows))
        .with_key_selector(key)
        .build();

    windows_reactor::vstack(vec![header.into(), body]).into()
}

fn header_cell<T>(c: &ResolvedColumn<T>, sort_state: Option<&SortState<String>>, on_sort: Option<&SetState<String>>) -> Element {
    let label = match sort_state {
        Some(s) if c.sortable && s.field_id.as_deref() == Some(c.id) => {
            format!("{} {}", c.header, if s.descending { "▼" } else { "▲" })
        }
        _ => c.header.clone(),
    };
    let text: Element = windows_reactor::text_block(label).into();
    let cell: Element = match (c.sortable, on_sort) {
        (true, Some(cb)) => {
            let id = c.id.to_string();
            let cb = cb.clone();
            windows_reactor::border(text).on_tapped(move || cb.call(id.clone())).into()
        }
        _ => text,
    };
    cell.width(c.width.get() as f64)
}

fn column_resize_handle(cx: &mut RenderCx, width: Signal<u64>, request_rerender: SetState<()>) -> Element {
    let current = width.get() as f64;
    let set = SetState::new(move |w: f64| {
        width.set(w as u64, None);
        request_rerender.call(());
    });
    resize_handle(cx, current, set).min(MIN_COLUMN_WIDTH).build()
}

fn row_view<T>(row: &T, columns: &[ResolvedColumn<T>]) -> Element {
    let cells: Vec<Element> = columns
        .iter()
        .map(|c| {
            let width = c.width.get() as f64;
            (c.cell)(row).width(width)
        })
        .collect();
    hstack(cells).into()
}

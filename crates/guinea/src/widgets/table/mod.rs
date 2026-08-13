mod flow;
mod layout;

pub use flow::{SortState, TableDataBuilder, TableFlowState, TableNode};
pub use layout::{IntoWidth, TableLayout, Width};

use std::rc::Rc;
use windows_reactor::{grid, hstack, Color, Element, ElementExt, GridLength, RenderCx, SetState, Shape, Thickness};

use crate::widgets::resize::resize_handle;

const MIN_COLUMN_WIDTH: f64 = 24.0;
const HEADER_SEPARATOR_COLOR: Color = Color { a: 48, r: 128, g: 128, b: 128 };

/// Horizontal inset shared by header and body cells so column content never
/// sits flush against the resize handle or the row edge. Opt out per column
/// with [`ColumnSpec::flush`].
const CELL_HORIZONTAL_PADDING: f64 = 12.0;
/// Extra vertical room for the header row - it can carry two-line content
/// (label + aggregate value) where body rows stay a single fixed height.
const HEADER_VERTICAL_PADDING: f64 = 8.0;

pub struct ColumnSpec<T> {
    pub id: &'static str,
    pub header: Rc<dyn Fn() -> Element>,
    pub initial_width: Width,
    pub min_width: f64,
    pub sortable: bool,
    pub flush: bool,
    pub cell: Rc<dyn Fn(&T) -> Element>,
}

impl<T> ColumnSpec<T> {
    pub fn new(id: &'static str, header: impl Into<String>, initial_width: impl IntoWidth, cell: impl Fn(&T) -> Element + 'static) -> Self {
        let header = header.into();
        Self { id, header: Rc::new(move || windows_reactor::text_block(header.clone()).into()), initial_width: initial_width.into_width(), min_width: MIN_COLUMN_WIDTH, sortable: false, flush: false, cell: Rc::new(cell) }
    }

    /// Use an arbitrary `Element` as the column header instead of plain text.
    /// The factory is called on every render, so the element can depend on signals.
    pub fn new_with_header(id: &'static str, header: impl Fn() -> Element + 'static, initial_width: impl IntoWidth, cell: impl Fn(&T) -> Element + 'static) -> Self {
        Self { id, header: Rc::new(header), initial_width: initial_width.into_width(), min_width: MIN_COLUMN_WIDTH, sortable: false, flush: false, cell: Rc::new(cell) }
    }

    /// Sets the minimum width enforced by the resize handle.
    pub fn min_width(mut self, min_width: f64) -> Self {
        self.min_width = min_width;
        self
    }

    /// Makes the header clickable and shows the sort indicator when `table`
    /// receives the matching `SortState`. The sort id is the column `id`.
    pub fn sortable(mut self) -> Self {
        self.sortable = true;
        self
    }

    /// Drops [`CELL_HORIZONTAL_PADDING`] for this column, header and body
    /// alike, so its content starts at the column's own left edge.
    ///
    /// For a column that reserves its own left gutter - a tree column with a
    /// chevron slot, say - the shared inset is a second gutter on top of the
    /// first, and the two of them push the content visibly off the table's
    /// edge. Per-column rather than global because the *other* columns still
    /// want it: without the inset, adjacent cells with a background (a heat
    /// wash) run into one another.
    pub fn flush(mut self) -> Self {
        self.flush = true;
        self
    }
}

struct ResolvedColumn<T> {
    id: &'static str,
    header: Rc<dyn Fn() -> Element>,
    width: Width,
    min_width: f64,
    sortable: bool,
    flush: bool,
    cell: Rc<dyn Fn(&T) -> Element>,
}

pub fn table<T: 'static>(
    cx: &mut RenderCx,
    rows: Vec<T>,
    columns: Vec<ColumnSpec<T>>,
    key: impl Fn(&T) -> String + 'static,
    sort: Option<(SortState<String>, SetState<String>)>,
    selection: Option<(i32, SetState<i32>)>,
) -> Element {
    table_with_sort_indicator(cx, rows, columns, key, sort, selection, None)
}

/// Like [`table`], but lets the caller render the sort direction indicator
/// itself (e.g. a themed icon) instead of the plain text glyph fallback.
/// `guinea` has no dependency on any particular icon set, so it cannot
/// bundle one - the closure receives `descending` and returns the element
/// shown next to the active sort column's header.
pub fn table_with_sort_indicator<T: 'static>(
    cx: &mut RenderCx,
    rows: Vec<T>,
    columns: Vec<ColumnSpec<T>>,
    key: impl Fn(&T) -> String + 'static,
    sort: Option<(SortState<String>, SetState<String>)>,
    selection: Option<(i32, SetState<i32>)>,
    sort_indicator: Option<Rc<dyn Fn(bool) -> Element>>,
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
                    min_width: spec.min_width,
                    sortable: spec.sortable,
                    flush: spec.flush,
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
        header_cells.push(header_cell(c, sort_state.as_ref(), on_sort.as_ref(), sort_indicator.as_ref()));
        if i != last_index {
            header_cells.push(column_resize_handle(cx, c.width.clone(), c.min_width, request_rerender.clone()));
        }
    }
    let header = hstack(header_cells);

    let columns_for_rows = columns.clone();
    let body = {
        let builder = windows_reactor::list_view(rows, move |row: &T, _idx: usize| row_view(row, &columns_for_rows))
            .with_key_selector(key);
        match selection {
            Some((selected_index, on_selection_changed)) => builder
                .selected_index(selected_index)
                .on_selection_changed(on_selection_changed)
                .build(),
            None => builder.build(),
        }
    };

    let header_separator: Element = Shape::rectangle()
        .fill(HEADER_SEPARATOR_COLOR)
        .height(1.0)
        .into();

    grid((header.grid_row(0), header_separator.grid_row(1), body.grid_row(2)))
        .rows([GridLength::Auto, GridLength::Auto, GridLength::Star(1.0)])
        .into()
}

fn header_cell<T>(
    c: &ResolvedColumn<T>,
    sort_state: Option<&SortState<String>>,
    on_sort: Option<&SetState<String>>,
    sort_indicator: Option<&Rc<dyn Fn(bool) -> Element>>,
) -> Element {
    let active = sort_state.filter(|s| c.sortable && s.field_id.as_deref() == Some(c.id));

    let base = (c.header)();
    let content: Element = match active {
        Some(s) => {
            let indicator = match sort_indicator {
                Some(render) => render(s.descending),
                None => windows_reactor::text_block(if s.descending { "▼" } else { "▲" }).into(),
            };
            hstack(vec![base, indicator]).into()
        }
        None => base,
    };

    let cell: Element = match (c.sortable, on_sort) {
        (true, Some(cb)) => {
            let id = c.id.to_string();
            let cb = cb.clone();
            windows_reactor::border(content).on_tapped(move || cb.call(id.clone())).into()
        }
        _ => content,
    };
    cell.padding(Thickness::xy(
        if c.flush { 0.0 } else { CELL_HORIZONTAL_PADDING },
        HEADER_VERTICAL_PADDING,
    ))
        .width(c.width.get() as f64)
}

fn column_resize_handle(cx: &mut RenderCx, width: Width, min_width: f64, request_rerender: SetState<()>) -> Element {
    let current = width.get() as f64;
    let set = SetState::new(move |w: f64| {
        width.set(w as u64);
        request_rerender.call(());
    });
    resize_handle(cx, current, set).min(min_width).build()
}

fn row_view<T>(row: &T, columns: &[ResolvedColumn<T>]) -> Element {
    let cells: Vec<Element> = columns
        .iter()
        .map(|c| {
            let width = c.width.get() as f64;
            (c.cell)(row)
                .padding(Thickness::xy(
                    if c.flush { 0.0 } else { CELL_HORIZONTAL_PADDING },
                    0.0,
                ))
                .width(width)
        })
        .collect();
    hstack(cells).into()
}

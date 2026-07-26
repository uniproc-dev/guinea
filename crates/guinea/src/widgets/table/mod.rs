mod flow;
mod layout;

pub use flow::{SortState, TableDataBuilder, TableFlowState, TableNode};
pub use layout::{IntoWidth, TableLayout};

use guinea_core::signal::Signal;
use std::rc::Rc;
use windows_reactor::{border, hstack, Color, Element, ElementExt, HorizontalAlignment, PointerEventInfo, RenderCx, SetState};

const RESIZE_HANDLE_WIDTH: f64 = 6.0;
const MIN_COLUMN_WIDTH: f64 = 24.0;

pub struct ColumnSpec<T> {
    pub id: &'static str,
    pub header: String,
    pub initial_width: Signal<u64>,
    pub cell: Rc<dyn Fn(&T) -> Element>,
}

impl<T> ColumnSpec<T> {
    pub fn new(id: &'static str, header: impl Into<String>, initial_width: impl IntoWidth, cell: impl Fn(&T) -> Element + 'static) -> Self {
        Self { id, header: header.into(), initial_width: initial_width.into_width(), cell: Rc::new(cell) }
    }
}

struct ResolvedColumn<T> {
    header: String,
    width: Signal<u64>,
    cell: Rc<dyn Fn(&T) -> Element>,
}

pub fn table<T: 'static>(cx: &mut RenderCx, rows: Vec<T>, columns: Vec<ColumnSpec<T>>, key: impl Fn(&T) -> String + 'static) -> Element {
    let layout_ref = cx.use_ref(TableLayout::<&'static str>::new());
    let columns: Rc<Vec<ResolvedColumn<T>>> = {
        let mut layout = layout_ref.borrow_mut();
        Rc::new(
            columns
                .into_iter()
                .map(|spec| ResolvedColumn {
                    width: layout.add_column(spec.id, spec.initial_width),
                    header: spec.header,
                    cell: spec.cell,
                })
                .collect(),
        )
    };

    // Column widths live in a `Signal` (amethystate), not windows-reactor's
    // own `use_state` - resizing a column doesn't otherwise ask the
    // reconciler to re-render. The drag handler below sets the signal and
    // requests a rerender in the same UI-thread callback, rather than
    // subscribing to the signal (its callback bound is `Send + Sync`, which
    // `SetState` - `Rc`-based, UI-thread-only - can't satisfy).
    let (_, request_rerender) = cx.use_state(());

    let last_index = columns.len().saturating_sub(1);
    let header_cells: Vec<Element> = columns
        .iter()
        .enumerate()
        .flat_map(|(i, c)| {
            let header: Element = windows_reactor::text_block(c.header.clone()).width(c.width.get() as f64).into();
            if i == last_index {
                vec![header]
            } else {
                vec![header, column_resize_handle(c.width.clone(), request_rerender.clone())]
            }
        })
        .collect();
    let header = hstack(header_cells);

    let columns_for_rows = columns.clone();
    let body = windows_reactor::list_view(rows, move |row: &T, _idx: usize| row_view(row, &columns_for_rows))
        .with_key_selector(key)
        .build();

    windows_reactor::vstack(vec![header.into(), body]).into()
}

/// A thin draggable strip between two header cells that resizes the column
/// to its left. Relies on windows-reactor auto-capturing the pointer for any
/// element that tracks `on_pointer_moved`, so `PointerMoved` keeps firing
/// past the strip's own (narrow) bounds mid-drag.
fn column_resize_handle(width: Signal<u64>, request_rerender: SetState<()>) -> Element {
    border(Element::Empty)
        .width(RESIZE_HANDLE_WIDTH)
        .background(Color { a: 40, r: 128, g: 128, b: 128 })
        .horizontal_alignment(HorizontalAlignment::Left)
        .on_pointer_pressed(|_: PointerEventInfo| {})
        .on_pointer_moved(move |info: PointerEventInfo| {
            if info.is_left_button_pressed {
                let new_width = (width.get() as f64 + info.x).max(MIN_COLUMN_WIDTH);
                width.set(new_width as u64, None);
                request_rerender.call(());
            }
        })
        .into()
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

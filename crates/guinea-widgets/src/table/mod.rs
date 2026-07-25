mod flow;
mod layout;

pub use flow::{SortState, TableDataBuilder, TableFlowState, TableNode};
pub use layout::{IntoWidth, TableLayout};

use guinea_core::signal::Signal;
use std::rc::Rc;
use windows_reactor::{Element, ElementExt, RenderCx, hstack, list_view};

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

    let header_cells: Vec<Element> = columns
        .iter()
        .map(|c| windows_reactor::text_block(c.header.clone()).width(c.width.get() as f64).into())
        .collect();
    let header = hstack(header_cells);

    let columns_for_rows = columns.clone();
    let body = list_view(rows, move |row: &T, _idx: usize| row_view(row, &columns_for_rows))
        .with_key_selector(key)
        .build();

    windows_reactor::vstack(vec![header.into(), body]).into()
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

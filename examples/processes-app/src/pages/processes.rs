use guinea::feature::FeatureInitContext;
use guinea::winui::{Page, PageCx};
use guinea::uri::AppUri;
use guinea_widgets::table::{ColumnSpec, table};
use windows_reactor::{Element, button, text_block, title, vstack};

use processes_core::processes::contracts::{Kill, ProcessesReducer};

pub struct Processes;

struct Row {
    pid: u32,
    label: String,
}

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::processes::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx) -> Element {
        let (state, dispatch) = cx.use_reducer::<ProcessesReducer>();

        let rows: Vec<Row> = state
            .items
            .iter()
            .map(|item| Row { pid: parse_pid(item), label: item.clone() })
            .collect();

        let columns = vec![
            ColumnSpec::new("name", "Process", 280, |row: &Row| text_block(row.label.clone()).into()),
            ColumnSpec::new("actions", "", 80, {
                let dispatch = dispatch.clone();
                move |row: &Row| {
                    let pid = row.pid;
                    let dispatch = dispatch.clone();
                    button("Kill").on_click(move || dispatch.emit(Kill(pid))).into()
                }
            }),
        ];

        vstack((title("Processes"), table(cx, rows, columns, |row: &Row| row.pid.to_string(), None, None)))
            .spacing(16.0)
            .into()
    }
}

fn parse_pid(row: &str) -> u32 {
    row.rsplit_once("(pid ")
        .and_then(|(_, rest)| rest.trim_end_matches(')').trim().parse().ok())
        .unwrap_or(0)
}

use guinea::feature::FeatureInitContext;
use guinea::router::{Page, PageCx};
use guinea::uri::AppUri;
use windows_reactor::{Element, button, hstack, text_block, title, vstack};

use super::contracts::ProcessesReducer;

pub struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        super::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx) -> Element {
        let (state, dispatch) = cx.use_reducer::<ProcessesReducer>();

        let rows: Vec<Element> = state
            .items
            .iter()
            .map(|row| {
                let pid = parse_pid(row);
                let dispatch = dispatch.clone();
                hstack((
                    text_block(row.clone()),
                    button("Kill").on_click(move || dispatch.emit_on_kill(pid)),
                ))
                .spacing(12.0)
                .into()
            })
            .collect();

        vstack((title("Processes"), vstack(rows).spacing(6.0)))
            .spacing(16.0)
            .into()
    }
}

fn parse_pid(row: &str) -> u32 {
    row.rsplit_once("(pid ")
        .and_then(|(_, rest)| rest.trim_end_matches(')').trim().parse().ok())
        .unwrap_or(0)
}

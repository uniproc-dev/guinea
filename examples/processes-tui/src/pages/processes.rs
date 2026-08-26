use guinea::feature::FeatureInitContext;
use guinea::ratatui::{Page, PageCx};
use guinea::uri::AppUri;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use processes_core::processes::contracts::ProcessesReducer;

use crate::cursor::{self, Cursor};

pub struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        ctx.seed_reducer::<Cursor>(0);
        processes_core::processes::install::install(ctx, uri)
    }

    fn render(cx: &mut PageCx<'_, '_>) {
        let (state, _) = cx.read::<ProcessesReducer>();
        let (cursor, _) = cx.read::<Cursor>();
        let area = cx.area();

        let focused = cursor::focused(cursor, state.items.len());
        let items: Vec<ListItem> = state
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let row = ListItem::new(format!(" {} {item}", i + 1));
                if i == focused {
                    row.style(Style::default().add_modifier(Modifier::REVERSED))
                } else {
                    row
                }
            })
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Processes ")
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        );
        cx.frame().render_widget(list, area);
    }
}

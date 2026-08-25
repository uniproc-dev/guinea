use guinea::feature::FeatureInitContext;
use guinea::ratatui::{Page, PageCx};
use guinea::uri::AppUri;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use processes_core::processes::contracts::ProcessesReducer;

pub struct Processes;

impl Page for Processes {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::processes::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx<'_, '_>) {
        let (state, _) = cx.read::<ProcessesReducer>();
        let area = cx.area();

        let items: Vec<ListItem> = state
            .items
            .iter()
            .enumerate()
            .map(|(i, item)| ListItem::new(format!(" {} {item}", i + 1)))
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

/// The pid a row carries, for the key handler that kills it.
///
/// The list is a `Vec<String>` shaped like "name (pid 42)" - the same strings
/// the WinUI table parses for its Kill button. Reading it back here keeps the
/// reducer's state identical between the two front ends.
pub fn pid_at(items: &[String], index: usize) -> Option<u32> {
    let row = items.get(index)?;
    row.rsplit_once("(pid ")
        .and_then(|(_, rest)| rest.trim_end_matches(')').trim().parse().ok())
}

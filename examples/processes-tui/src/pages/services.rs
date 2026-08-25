use guinea::feature::FeatureInitContext;
use guinea::ratatui::{Page, PageCx};
use guinea::uri::AppUri;
use ratatui::style::{Modifier, Style};
use ratatui::widgets::{Block, Borders, List, ListItem};

use processes_core::services::contracts::ServicesReducer;

pub struct Services;

impl Page for Services {
    fn install(ctx: &FeatureInitContext, uri: &AppUri) -> anyhow::Result<()> {
        processes_core::services::install::install(ctx, uri)
    }

    fn view(cx: &mut PageCx<'_, '_>) {
        let (state, _) = cx.read::<ServicesReducer>();
        let area = cx.area();

        let items: Vec<ListItem> = state
            .items
            .iter()
            .map(|item| ListItem::new(format!(" {item}")))
            .collect();

        let list = List::new(items).block(
            Block::default()
                .borders(Borders::ALL)
                .title(" Services ")
                .title_style(Style::default().add_modifier(Modifier::BOLD)),
        );
        cx.frame().render_widget(list, area);
    }
}

#![cfg(feature = "winui")]

//! A WinUI page under the harness: mounted with no window, typed into and
//! clicked, and read back as the tree it drew - with the features behind it
//! running in the seed's order.

use guinea::app::Harness;
use guinea::feature::Segment;
use guinea::prelude::*;
use guinea::winui::harness::Mounted;
use guinea::winui::{MarkExt, Page, PageCx, UpdateCx, page};
use windows_reactor::{
    Button, ChildrenControl, ContentControl, ItemsRepeater, StackPanel, TextBlock, View,
    VirtualSource,
};

const CATALOGUE: [&str; 6] = ["guinea", "guinea-app", "gui", "gum", "gulp", "gust"];

#[derive(guinea::Mark)]
enum Marks {
    Search,
    Open,
}

#[derive(Default, Clone, PartialEq, Debug)]
pub struct Results {
    pub query: String,
    pub found: Vec<&'static str>,
}

#[derive(Clone, Debug)]
pub struct Found {
    pub query: String,
    pub found: Vec<&'static str>,
}

impl Reducer for Results {
    type Update = Found;

    fn reduce(&mut self, update: Found) {
        self.query = update.query;
        self.found = update.found;
    }
}

#[derive(Clone, Debug)]
pub struct Query(pub String);

#[derive(Debug)]
pub struct Searcher {
    pub push: Push<Results>,
}

actor! {
    Searcher {
        handlers { Query => { bg Found }, Found }
    }
}

#[handler]
fn query(_this: &mut Searcher, ctx: Context<Searcher, Query>) {
    let text = ctx.msg.0.clone();
    ctx.spawn_bg::<Found, _>(async move {
        guinea::core::executor::random_delay().await;
        let found = CATALOGUE.iter().copied().filter(|name| name.contains(text.as_str())).collect();
        Found { query: text, found }
    });
}

#[handler]
fn found(this: &mut Searcher, ctx: Context<Searcher, Found>) {
    this.push.send(ctx.msg.clone());
}

pub struct Search {
    _results: Bound<Results>,
}

#[installs]
impl Feature for Search {
    type Exports = (Results,);

    fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
        let (results, _) = cx.state::<Results>().driven_by(|push| Searcher { push });
        Ok(Self { _results: results })
    }
}

/// A box to type into, a button, and what the last search found.
#[derive(Default)]
pub struct SearchPage {
    typed: String,
}

pub enum Typing {
    Typed(String),
}

#[page]
impl Page for SearchPage {
    type Params = ();
    type Installs = Search;
    type Message = Typing;

    fn install(ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<Search> {
        ctx.install(&())
    }

    fn update(&mut self, message: Typing, _cx: &mut UpdateCx<'_, Self>) {
        match message {
            Typing::Typed(text) => self.typed = text,
        }
    }

    fn view(&self, cx: &mut PageCx<'_, Self>) -> View {
        let (results, dispatch) = cx.use_reducer::<Results, _>();
        let typed = self.typed.clone();

        StackPanel::new().children((
            TextBlock::new().text(format!("typed: {}", self.typed)),
            Button::new()
                .mark(Marks::Search)
                .on_click(move || dispatch.emit(Query(typed.clone())))
                .content(TextBlock::new().text("Search")),
            TextBlock::new().text(format!("found {} for {:?}", results.found.len(), results.query)),
        ))
        .into()
    }
}

/// What `routes!` would write: the page stands alone.
impl Segment for SearchPage {
    type Installs = Search;
    type Above = ();
}

#[guinea::test(iterations = 30)]
fn typing_and_clicking_search_draws_what_it_found(h: &mut Harness) {
    let mut page = Mounted::<SearchPage>::mount(&h.segment(), ()).unwrap();
    assert!(page.find_text("found 0 for \"\"").is_some(), "{:#?}", page.tree());

    page.send(Typing::Typed("guinea".into()));
    page.click(Marks::Search);
    page.settle();

    assert!(page.find_text("typed: guinea").is_some(), "{:#?}", page.tree());
    assert!(page.find_text("found 2 for \"guinea\"").is_some(), "{:#?}", page.tree());
}

#[guinea::test(iterations = 4)]
fn the_tree_a_page_draws_is_data(h: &mut Harness) {
    let mut page = Mounted::<SearchPage>::mount(&h.segment(), ()).unwrap();
    page.send(Typing::Typed("gu".into()));
    page.click(Marks::Search);
    page.settle();

    insta::assert_ron_snapshot!("search_page", page.tree());
}

/// The whole catalogue as a list, each item with a button that opens it.
#[derive(Default)]
pub struct CataloguePage {
    opened: Option<&'static str>,
}

pub enum Browsing {
    Opened(usize),
}

#[page]
impl Page for CataloguePage {
    type Params = ();
    type Installs = ();
    type Message = Browsing;

    fn install(_ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<()> {
        Ok(())
    }

    fn update(&mut self, message: Browsing, _cx: &mut UpdateCx<'_, Self>) {
        match message {
            Browsing::Opened(at) => self.opened = Some(CATALOGUE[at]),
        }
    }

    fn view(&self, cx: &mut PageCx<'_, Self>) -> View {
        let open = cx.on(Browsing::Opened);

        let items = VirtualSource::new(
            CATALOGUE.len() as u64,
            CATALOGUE.len(),
            |index| index,
            move |index| {
                let open = open.clone();

                StackPanel::new()
                    .children((
                        TextBlock::new().text(CATALOGUE[index]),
                        Button::new()
                            .mark(Marks::Open)
                            .on_click(move || {
                                let _ = open.call(index);
                            })
                            .content(TextBlock::new().text("Open")),
                    ))
            },
        );

        StackPanel::new()
            .children((
                TextBlock::new().text(format!("opened: {}", self.opened.unwrap_or("nothing"))),
                ItemsRepeater::new().virtual_source(items),
            ))
            .into()
    }
}

impl Segment for CataloguePage {
    type Installs = ();
    type Above = ();
}

#[guinea::test(iterations = 4)]
fn the_same_mark_in_every_item_is_found_within_the_item(h: &mut Harness) {
    let mut page = Mounted::<CataloguePage>::mount(&h.segment(), ()).unwrap();
    assert!(page.find(Marks::Open).is_none(), "no item is built before it is in view");

    page.item(3).click(Marks::Open);
    page.settle();
    assert!(page.find_text("opened: gum").is_some(), "{:#?}", page.tree());

    let item = page.item(1);
    assert!(item.find_text("guinea-app").is_some(), "{:#?}", item.tree());
    item.click(Marks::Open);
    page.settle();
    assert!(page.find_text("opened: guinea-app").is_some(), "{:#?}", page.tree());
}

/// Samples on a timer; installed into the segment above the page.
mod polling {
    use super::*;

    #[derive(Default, Clone, PartialEq, Debug)]
    pub struct Samples {
        pub taken: u32,
    }

    #[derive(Clone, Debug)]
    pub struct Taken;

    impl Reducer for Samples {
        type Update = Taken;

        fn reduce(&mut self, _taken: Taken) {
            self.taken += 1;
        }
    }

    #[derive(Clone, Debug)]
    pub struct Sample;

    #[derive(Debug)]
    pub struct Sampler {
        pub push: Push<Samples>,
    }

    actor! {
        Sampler {
            handlers { Sample }
        }
    }

    #[handler]
    fn sample(this: &mut Sampler, _ctx: Context<Sampler, Sample>) {
        this.push.send(Taken);
    }

    pub struct Polling {
        _samples: Bound<Samples>,
    }

    #[installs]
    impl Feature for Polling {
        type Exports = (Samples,);

        fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
            let (samples, sampler) = cx.state::<Samples>().driven_by(|push| Sampler { push });
            cx.every(std::time::Duration::from_millis(100), &sampler, || Sample);
            Ok(Self { _samples: samples })
        }
    }
}

/// The segment above the page, as `routes!` would name it.
pub struct Shell;

impl Segment for Shell {
    type Installs = polling::Polling;
    type Above = ();
}

/// Shows how many samples the segment above it has taken.
#[derive(Default)]
pub struct SamplesPage;

#[page]
impl Page for SamplesPage {
    type Params = ();
    type Installs = ();
    type Message = ();

    fn install(_ctx: &FeatureInitContext, _params: &()) -> anyhow::Result<()> {
        Ok(())
    }

    fn update(&mut self, _message: (), _cx: &mut UpdateCx<'_, Self>) {}

    fn view(&self, cx: &mut PageCx<'_, Self>) -> View {
        let (samples, _) = cx.use_reducer::<polling::Samples, _>();
        TextBlock::new().text(format!("samples: {}", samples.taken)).into()
    }
}

impl Segment for SamplesPage {
    type Installs = ();
    type Above = (Shell, ());
}

#[guinea::test(iterations = 8)]
fn a_page_redraws_when_the_segment_above_it_changes(h: &mut Harness) {
    h.install::<polling::Polling>(&()).unwrap();

    let below = h.child();
    let mut page = Mounted::<SamplesPage>::mount(&below, ()).unwrap();
    assert!(page.find_text("samples: 0").is_some(), "{:#?}", page.tree());

    h.advance(std::time::Duration::from_millis(300));
    page.settle();

    assert!(page.find_text("samples: 3").is_some(), "{:#?}", page.tree());
}

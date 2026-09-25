//! The harness on the race it exists for: two searches in flight, and the
//! answer that lands last decides what the page shows.
//!
//! The user types `gu`, then `guinea`. A search takes as long as it takes, so
//! the one for `gu` can land after the one for `guinea` - and a searcher that
//! shows whatever arrived last shows results for a query nobody is looking at
//! any more. Under tokio that happens in production and almost never in a
//! test; here the seeds walk the orders until one does.

use guinea::app::Harness;
use guinea::prelude::*;

const CATALOGUE: [&str; 6] = ["guinea", "guinea-app", "gui", "gum", "gulp", "gust"];

#[derive(Default, Clone, PartialEq, Debug, serde::Serialize)]
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

/// Takes as long as the seed says.
async fn search(text: String) -> Found {
    let found: Vec<&'static str> = CATALOGUE
        .iter()
        .copied()
        .filter(|name| name.contains(text.as_str()))
        .collect();

    guinea::core::executor::random_delay().await;

    Found { query: text, found }
}

/// Shows whatever answer arrives, in the order they arrive.
mod naive {
    use super::*;

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
        ctx.spawn_bg::<Found, _>(search(text));
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
}

/// Numbers every query and shows an answer only if it is for the latest.
mod latest {
    use super::*;

    #[derive(Debug)]
    pub struct Searcher {
        pub push: Push<Results>,
        pub asked: u64,
    }

    #[derive(Clone, Debug)]
    pub struct Answered {
        pub asked: u64,
        pub found: Found,
    }

    actor! {
        Searcher {
            handlers { Query => { bg Answered }, Answered }
        }
    }

    #[handler]
    fn query(this: &mut Searcher, ctx: Context<Searcher, Query>) {
        this.asked += 1;

        let asked = this.asked;
        let text = ctx.msg.0.clone();
        ctx.spawn_bg::<Answered, _>(async move {
            Answered {
                asked,
                found: search(text).await,
            }
        });
    }

    #[handler]
    fn answered(this: &mut Searcher, ctx: Context<Searcher, Answered>) {
        if ctx.msg.asked == this.asked {
            this.push.send(ctx.msg.found.clone());
        }
    }

    pub struct Search {
        _results: Bound<Results>,
    }

    #[installs]
    impl Feature for Search {
        type Exports = (Results,);

        fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
            let (results, _) =
                cx.state::<Results>().driven_by(|push| Searcher { push, asked: 0 });
            Ok(Self { _results: results })
        }
    }
}

fn type_gu_then_guinea(h: &Harness) {
    h.dispatch::<Results>().emit(Query("gu".into()));
    h.dispatch::<Results>().emit(Query("guinea".into()));
    h.settled();
}

#[guinea::test(iterations = 200)]
#[should_panic(expected = "SEED=")]
fn some_order_shows_an_answer_to_a_query_nobody_is_looking_at(h: &mut Harness) {
    h.install::<naive::Search>(&()).unwrap();
    type_gu_then_guinea(h);

    assert_eq!(h.state::<Results>().query, "guinea");
}

#[guinea::test(iterations = 200)]
fn in_every_order_the_latest_query_wins(h: &mut Harness) {
    h.install::<latest::Search>(&()).unwrap();
    type_gu_then_guinea(h);

    let results = h.state::<Results>();
    assert_eq!(results.query, "guinea");
    assert_eq!(results.found, ["guinea", "guinea-app"]);
    assert_eq!(h.stuck(), 0);
}

/// Waits the way the example's metrics do: `tokio::time::sleep` inside the
/// background task.
mod sleepy {
    use super::*;

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
            guinea::core::__private::tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            Found { query: text, found: Vec::new() }
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
}

#[guinea::test(iterations = 8)]
fn a_tokio_sleep_in_the_code_under_test_waits_on_the_test_clock(h: &mut Harness) {
    let started = std::time::Instant::now();

    h.install::<sleepy::Search>(&()).unwrap();
    h.dispatch::<Results>().emit(Query("gu".into()));

    h.settled();
    assert_eq!(h.state::<Results>().query, "", "answered before its sleep was over");
    assert_eq!(h.stuck(), 1);

    h.advance(std::time::Duration::from_millis(799));
    assert_eq!(h.state::<Results>().query, "", "a millisecond early");

    h.advance(std::time::Duration::from_millis(1));
    assert_eq!(h.state::<Results>().query, "gu");
    assert_eq!(h.stuck(), 0);

    assert!(started.elapsed() < std::time::Duration::from_millis(200), "waited in real time");
}

/// Samples on a timer, the way the example's metrics feature polls.
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

#[guinea::test(iterations = 4)]
fn a_feature_timer_ticks_on_the_test_clock(h: &mut Harness) {
    h.install::<polling::Polling>(&()).unwrap();

    h.advance(std::time::Duration::from_millis(99));
    assert_eq!(h.state::<polling::Samples>().taken, 0);

    h.advance(std::time::Duration::from_millis(901));
    assert_eq!(h.state::<polling::Samples>().taken, 10);
}

/// A poll that never ends runs beside the search. Waiting for everything to
/// go quiet would never return; waiting for what the query set off does, and
/// the chain it hands back says what that was.
#[guinea::test(iterations = 50)]
fn settling_one_action_waits_for_its_own_work_and_no_more(h: &mut Harness) {
    h.install::<polling::Polling>(&()).unwrap();
    h.install::<sleepy::Search>(&()).unwrap();

    let asked = h.act::<Results>(Query("gu".into()));
    assert!(!asked.is_settled());

    asked.settle();
    assert_eq!(h.state::<Results>().query, "gu");

    let chain = asked.chain();
    assert!(chain.handled::<Query>());
    assert!(chain.handled::<Found>());
    assert!(chain.pushed::<Results>());
    assert!(!chain.cancelled());

    let taken = h.state::<polling::Samples>().taken;
    assert!(
        (7..=8).contains(&taken),
        "the clock went 800 ms for the search, and the poll ran on it: {taken}"
    );
}

/// The shape of what a query sets off, and where it leaves the state: a
/// refactor that drops the push, or sends the answer round an extra actor,
/// changes the first; one that reorders the results changes the second.
#[guinea::test(iterations = 50)]
fn what_a_query_sets_off_keeps_its_shape(h: &mut Harness) {
    h.install::<latest::Search>(&()).unwrap();

    let asked = h.act::<Results>(Query("guinea".into()));
    asked.settle();

    insta::assert_ron_snapshot!("a_query", asked.chain().shape());
    insta::assert_ron_snapshot!("its_results", *h.state::<Results>());
}

/// A page's own count of what the layout above it sampled, kept by observing
/// the layout's reducer.
mod echo {
    use super::*;

    #[derive(Default, Clone, PartialEq, Debug)]
    pub struct Seen {
        pub samples: u32,
    }

    #[derive(Clone, Debug)]
    pub struct Saw;

    impl Reducer for Seen {
        type Update = Saw;

        fn reduce(&mut self, _saw: Saw) {
            self.samples += 1;
        }
    }

    pub struct Echo {
        _seen: Bound<Seen>,
    }

    #[installs]
    impl Feature for Echo {
        type Exports = (Seen,);

        fn install(cx: &FeatureInitContext, _params: &()) -> anyhow::Result<Self> {
            let seen = cx.state::<Seen>().plain();

            let pushing = seen.clone();
            cx.observe::<polling::Samples>(move |_taken| pushing.push(Saw));

            Ok(Self { _seen: seen })
        }
    }
}

fn ms(millis: u64) -> std::time::Duration {
    std::time::Duration::from_millis(millis)
}

#[guinea::test(iterations = 8)]
fn a_page_reads_and_hears_what_the_layout_above_it_exports(h: &mut Harness) {
    h.install::<polling::Polling>(&()).unwrap();

    let page = h.child();
    page.install::<echo::Echo>(&()).unwrap();

    h.advance(ms(300));
    assert_eq!(page.state::<polling::Samples>().taken, 3, "read from the layout");
    assert_eq!(page.state::<echo::Seen>().samples, 3, "heard every sample");
}

#[guinea::test(iterations = 50)]
fn leaving_a_page_mid_request_ends_its_work_and_only_its_work(h: &mut Harness) {
    h.install::<polling::Polling>(&()).unwrap();

    let page = h.child();
    page.install::<sleepy::Search>(&()).unwrap();

    let asked = page.act::<Results>(Query("gu".into()));
    h.advance(ms(400));
    page.leave();

    asked.settle();
    let chain = asked.chain();
    assert!(chain.cancelled(), "the search outlived its page:\n{chain:#?}");
    assert!(!chain.pushed::<Results>());

    h.advance(ms(600));
    assert_eq!(h.state::<polling::Samples>().taken, 10, "the layout's poll stopped with the page");
}

#[test]
fn a_seed_is_one_order_every_time() {
    let shown = |seed| {
        let h = Harness::new(seed);
        h.install::<naive::Search>(&()).unwrap();
        type_gu_then_guinea(&h);
        h.state::<Results>().query.clone()
    };

    let first: Vec<String> = (0..64).map(shown).collect();
    let again: Vec<String> = (0..64).map(shown).collect();
    assert_eq!(first, again);

    assert!(
        first.iter().any(|query| query == "gu") && first.iter().any(|query| query == "guinea"),
        "the seeds reach both orders: {first:?}"
    );
}

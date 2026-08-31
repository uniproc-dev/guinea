use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

mod actor_dsl;
mod elm;
mod handler;
mod installs;
mod routes_dsl;
mod segment;

/// Reads a feature's `Params` off its `install`, instead of asking for it
/// twice.
///
/// `type Params = str;` beside `fn install(cx, context: &str)` is one fact
/// written in two places, and only one of them is load-bearing: the body uses
/// the argument, so the signature cannot silently be wrong while the
/// associated type can.
///
/// `Exports` stays written down. What a feature publishes is a decision rather
/// than a consequence - it may claim four reducers and export one - so there
/// is nothing to read it from.
///
/// Named for the method rather than the trait: `#[feature]` is ambiguous with
/// Rust's own `feature` attribute.
///
/// ```ignore
/// #[installs]
/// impl Feature for Tabs {
///     type Exports = (contracts::Tabs,);
///
///     fn install(cx: &FeatureInitContext, context: &str) -> anyhow::Result<Self> { .. }
/// }
/// ```
#[proc_macro_attribute]
pub fn installs(_attr: TokenStream, item: TokenStream) -> TokenStream {
    installs::installs_impl(item)
}

/// Writes `type Installs = ();` and the `install` that goes with it, for a
/// page or layout that installs nothing.
///
/// Only that. Declaring what a segment installs is what makes the declaration
/// an obligation of the body; declaring that it installs *nothing* is
/// ceremony, and stable Rust has no conditional default body to remove it.
///
/// ```ignore
/// #[segment]
/// impl Page for Splash {
///     type Params = ();
///     fn view(cx: &mut PageCx<'_>) { .. }
/// }
/// ```
#[proc_macro_attribute]
pub fn segment(_attr: TokenStream, item: TokenStream) -> TokenStream {
    segment::segment_impl(item)
}

/// Writes down what an `impl Page` for the iced backend left out.
///
/// ```ignore
/// #[page]
/// impl Page for Services {
///     type Params = ServicesParams;
///
///     fn install(ctx: &FeatureInitContext, _: &Self::Params) -> anyhow::Result<()> { .. }
///     fn view(&self, cx: &PageCx<'_>) -> View<Self::Message> { .. }
/// }
/// ```
///
/// An omitted `Params` becomes `()` and an omitted `Message` becomes
/// `Infallible`; a node with the defaulted message also gets the empty
/// `update` that goes with it. Nothing else - a macro that derived a
/// declaration from a body would be a second source of truth wearing the
/// clothes of one.
#[proc_macro_attribute]
pub fn iced_page(_attr: TokenStream, item: TokenStream) -> TokenStream {
    elm::node_impl(item, elm::Kind::Page, "guinea-iced", "iced")
}

/// [`iced_page`] for a layout, which has no route parameters of its own.
#[proc_macro_attribute]
pub fn iced_layout(_attr: TokenStream, item: TokenStream) -> TokenStream {
    elm::node_impl(item, elm::Kind::Layout, "guinea-iced", "iced")
}

/// [`iced_page`] for the windows-reactor backend.
///
/// The same macro because the two backends are the same kind of thing: the
/// reactor's second preview is Elm - state in structs, events as enums - so a
/// page there has the same five items a page here does, and leaving out the
/// empty ones is the same job.
#[proc_macro_attribute]
pub fn winui_page(_attr: TokenStream, item: TokenStream) -> TokenStream {
    elm::node_impl(item, elm::Kind::Page, "guinea-winui", "winui")
}

/// [`winui_page`] for a layout.
#[proc_macro_attribute]
pub fn winui_layout(_attr: TokenStream, item: TokenStream) -> TokenStream {
    elm::node_impl(item, elm::Kind::Layout, "guinea-winui", "winui")
}

/// Declares an actor's manifest:
///
/// ```ignore
/// actor! {
///     ProcessActor<P: ProcessesPort + 'static> {
///         handlers   { Kill, Refresh }
///         publishes  { ProcessKilled }
///         subscribes { SettingsChanged }
///     }
/// }
/// ```
#[proc_macro]
pub fn actor(input: TokenStream) -> TokenStream {
    actor_dsl::actor_impl(input)
}

/// `routes! { Route { layout(TabsLayout) { page(Processes, "/:context/processes")
/// { context: String } ... } } }` - the tree's `{}` nesting *is* the segment
/// chain (no attribute stack to track); `page(...)`'s type also names the
/// generated variant, so there's one name per leaf, not two kept in sync by
/// hand. Generates the enum itself plus `path`/`parse` (string <-> enum),
/// `RouteChain` (enum -> segment chain), and `ToUri` (enum -> `AppUri`, just
/// the generated `.path()` string parsed - no per-app glue needed).
#[proc_macro]
pub fn routes(input: TokenStream) -> TokenStream {
    routes_dsl::routes_impl(input)
}

#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    handler::generate_standalone_handler(input)
}

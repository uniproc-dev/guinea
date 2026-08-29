use proc_macro::TokenStream;
use syn::{ItemFn, parse_macro_input};

mod actor_dsl;
mod handler;
mod iced_node;
mod routes_dsl;
mod segment;

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
    iced_node::node_impl(item, iced_node::Kind::Page)
}

/// [`iced_page`] for a layout, which has no route parameters of its own.
#[proc_macro_attribute]
pub fn iced_layout(_attr: TokenStream, item: TokenStream) -> TokenStream {
    iced_node::node_impl(item, iced_node::Kind::Layout)
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

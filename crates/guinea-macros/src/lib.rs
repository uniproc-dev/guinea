use proc_macro::TokenStream;
use syn::{ItemFn, ItemTrait, parse_macro_input};

mod actor_dsl;
mod contract_macros;
mod features;
mod handler;
mod reducer;
mod routes_dsl;

#[proc_macro_attribute]
pub fn port(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let trait_item = parse_macro_input!(item as ItemTrait);
    contract_macros::port_impl(trait_item)
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

#[proc_macro_attribute]
pub fn app_feature(args: TokenStream, input: TokenStream) -> TokenStream {
    features::app_feature_impl(args, input)
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
pub fn reducer(_attr: TokenStream, input: TokenStream) -> TokenStream {
    reducer::reducer_impl(input)
}

#[proc_macro_attribute]
pub fn dispatch(_attr: TokenStream, input: TokenStream) -> TokenStream {
    input
}

#[proc_macro_derive(ReducerState)]
pub fn derive_reducer_state(input: TokenStream) -> TokenStream {
    reducer::reducer_state_derive_impl(input)
}

#[proc_macro_attribute]
pub fn handler(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemFn);
    handler::generate_standalone_handler(input)
}

use proc_macro::TokenStream;
use syn::{ItemFn, ItemImpl, ItemTrait, parse_macro_input};

mod adapter;
mod binder_gen;
mod capability;
mod contract_macros;
mod dto;
mod features;
mod handler;
mod reducer;
mod routable;

#[proc_macro_attribute]
pub fn port(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let trait_item = parse_macro_input!(item as ItemTrait);
    contract_macros::port_impl(trait_item)
}

#[proc_macro_attribute]
pub fn actions(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let trait_item = parse_macro_input!(item as ItemTrait);
    contract_macros::actions_impl(trait_item)
}

#[proc_macro_attribute]
pub fn actor_manifest(attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as ItemImpl);
    guinea_codegen::actor_manifest::actor_manifest_impl(attr.into(), impl_block, std::iter::empty())
        .into()
}

#[proc_macro_attribute]
pub fn port_adapter(attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as ItemImpl);
    adapter::port_adapter_impl(attr, impl_block)
}

#[proc_macro_attribute]
pub fn bindings_adapter(attr: TokenStream, item: TokenStream) -> TokenStream {
    let impl_block = parse_macro_input!(item as ItemImpl);
    adapter::bindings_adapter_impl(attr, impl_block)
}

#[proc_macro_attribute]
pub fn capability(attr: TokenStream, item: TokenStream) -> TokenStream {
    capability::capability_impl(attr, item)
}

#[proc_macro_attribute]
pub fn slint_dto(_attr: TokenStream, item: TokenStream) -> TokenStream {
    dto::slint_dto_impl(item)
}

#[proc_macro_attribute]
pub fn window_feature(args: TokenStream, input: TokenStream) -> TokenStream {
    features::window_feature_impl(args, input)
}

#[proc_macro_attribute]
pub fn app_feature(args: TokenStream, input: TokenStream) -> TokenStream {
    features::app_feature_impl(args, input)
}

#[proc_macro_derive(Routable, attributes(route, layout, end_layout))]
pub fn derive_routable(input: TokenStream) -> TokenStream {
    routable::derive_routable_impl(input)
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

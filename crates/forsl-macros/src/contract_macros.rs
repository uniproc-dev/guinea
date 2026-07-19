use proc_macro::TokenStream;
use syn::ItemTrait;

use crate::binder_gen;

const HELPER_ATTRS: &[&str] = &["manual", "tracing", "slint"];

fn strip_helper_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| {
        !HELPER_ATTRS
            .iter()
            .any(|helper| attr.path().is_ident(helper))
    });
}

/// `#[port]` - backend-agnostic. Strips helper attrs and registers the trait
/// (+ a compile-time proof of the `UiXPort` -> `UiXPortMsg` naming
/// convention) via `forsl_codegen::contracts::emit_port_registration`, so
/// `domain-test-kit`'s stub generator can discover it later without any
/// source re-parsing.
pub fn port_impl(mut trait_item: ItemTrait) -> TokenStream {
    let registration = forsl_codegen::contracts::emit_port_registration(&trait_item);

    for item in &mut trait_item.items {
        if let syn::TraitItem::Fn(method) = item {
            strip_helper_attrs(&mut method.attrs);
        }
    }

    quote::quote! {
        #trait_item
        #registration
    }
    .into()
}

/// `#[bindings]` - backend-agnostic. Same as `#[port]`, plus keeps generating
/// the actor-side `<Feature>Binder`/`<Feature>PartialBinder` helpers.
pub fn bindings_impl(mut trait_item: ItemTrait) -> TokenStream {
    let registration = forsl_codegen::contracts::emit_binding_registration(&trait_item);
    let binder_code = binder_gen::generate_binder(&trait_item);

    for item in &mut trait_item.items {
        if let syn::TraitItem::Fn(method) = item {
            strip_helper_attrs(&mut method.attrs);
        }
    }

    quote::quote! {
        #trait_item
        #binder_code
        #registration
    }
    .into()
}

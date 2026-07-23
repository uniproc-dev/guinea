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
/// convention) via `guinea_codegen::contracts::emit_port_registration`, so
/// `domain-test-kit`'s stub generator can discover it later without any
/// source re-parsing.
///
/// Also emits a blanket impl for any `Fn(Msg)` closure - a port trait is
/// exactly one `send(&self, msg)` method (query-style extra methods are a
/// deprecated exception, not the shape this targets), so any closure of the
/// right signature already satisfies it. This means no per-feature adapter
/// struct is needed to wire a port to wherever its messages actually go
/// (e.g. `Store::push`) - the call site just writes
/// `move |msg| store.push::<F>(msg)` and hands that straight to the actor,
/// same as it always handed the actor a concrete adapter object.
pub fn port_impl(mut trait_item: ItemTrait) -> TokenStream {
    let registration = guinea_codegen::contracts::emit_port_registration(&trait_item);
    let trait_ident = &trait_item.ident;

    for item in &mut trait_item.items {
        if let syn::TraitItem::Fn(method) = item {
            strip_helper_attrs(&mut method.attrs);
        }
    }

    let msg_ty = trait_item.items.iter().find_map(|item| {
        let syn::TraitItem::Fn(method) = item else {
            return None;
        };
        if method.sig.ident != "send" {
            return None;
        }
        method.sig.inputs.iter().find_map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => Some((*pat_ty.ty).clone()),
            _ => None,
        })
    });

    let blanket_impl = msg_ty.map(|msg_ty| {
        quote::quote! {
            impl<__F: Fn(#msg_ty) + 'static> #trait_ident for __F {
                fn send(&self, msg: #msg_ty) {
                    self(msg)
                }
            }
        }
    });

    quote::quote! {
        #trait_item
        #blanket_impl
        #registration
    }
    .into()
}

/// `#[bindings]` - backend-agnostic. Same as `#[port]`, plus keeps generating
/// the actor-side `<Feature>Binder`/`<Feature>PartialBinder` helpers.
pub fn bindings_impl(mut trait_item: ItemTrait) -> TokenStream {
    let registration = guinea_codegen::contracts::emit_binding_registration(&trait_item);
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

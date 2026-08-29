//! `#[page]` and `#[layout]` for the iced adapter.
//!
//! One job: write down what the author did not.
//!
//! Stable Rust has no defaults for associated types, so a page that captures
//! nothing and has no messages of its own still has to say `type Params = ();`
//! and `type Message = ..;` and write the empty `update` that goes with them.
//! That is three lines saying nothing, and nothing but a macro can remove
//! them.
//!
//! Deliberately *only* that. Anything a type can check, a type should check -
//! a macro that derives a list from a body is a second source of truth that
//! looks like one source. What a node watches needs no such help: the
//! translation is named at the `cx.on(..)` call and nowhere else, so one that
//! is never registered is a function nobody calls, and the dead-code lint
//! already says so.

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{ImplItem, ItemImpl, parse_quote};

#[derive(Clone, Copy, PartialEq)]
pub enum Kind {
    Page,
    Layout,
}

/// Where the iced adapter's own types live.
///
/// An application reaches them through the facade and a test inside the
/// adapter does not, and the generated `update` has to name `UpdateCx` either
/// way.
fn adapter_path() -> TokenStream {
    match crate_name("guinea-iced") {
        Ok(FoundCrate::Itself) => return quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            return quote!(::#ident);
        }
        Err(_) => {}
    }

    match crate_name("guinea") {
        Ok(FoundCrate::Itself) => quote!(crate::iced),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident::iced)
        }
        Err(_) => quote!(::guinea::iced),
    }
}

pub fn node_impl(item: TokenStream1, kind: Kind) -> TokenStream1 {
    let mut item = syn::parse_macro_input!(item as ItemImpl);
    let adapter = adapter_path();

    let declared_type = |name: &str| {
        item.items
            .iter()
            .any(|entry| matches!(entry, ImplItem::Type(ty) if ty.ident == name))
    };
    let declared_fn = |name: &str| {
        item.items
            .iter()
            .any(|entry| matches!(entry, ImplItem::Fn(f) if f.sig.ident == name))
    };

    let has_params = declared_type("Params");
    let has_installs = declared_type("Installs");
    let has_message = declared_type("Message");
    let has_update = declared_fn("update");
    let has_install = declared_fn("install");

    if kind == Kind::Page && !has_params {
        item.items.push(parse_quote!(
            type Params = ();
        ));
    }

    if !has_installs {
        item.items.push(parse_quote!(
            type Installs = ();
        ));

        // Only alongside the defaulted list: a segment that named what it
        // installs and then left `install` out has forgotten to install it,
        // and the missing trait item says so.
        if !has_install {
            item.items.push(parse_quote! {
                fn install(
                    _ctx: &#adapter::FeatureInitContext,
                    _params: &Self::Params,
                ) -> ::anyhow::Result<()> {
                    ::std::result::Result::Ok(())
                }
            });
        }
    }

    if !has_message {
        item.items.push(parse_quote!(
            type Message = ::core::convert::Infallible;
        ));

        // Only alongside the defaulted message: a node that named its own and
        // then left `update` out has forgotten something, and the missing
        // trait item is a better error than an `update` that swallows.
        if !has_update {
            item.items.push(parse_quote! {
                fn update(
                    &mut self,
                    message: Self::Message,
                    _cx: &mut #adapter::UpdateCx<'_, Self>,
                ) {
                    match message {}
                }
            });
        }
    }

    quote!(#item).into()
}

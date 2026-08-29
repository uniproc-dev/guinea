//! `#[installs]` - `Params` read off `install`, rather than declared beside it.
//!
//! Named for the method it reads rather than for the trait it sits on, because
//! `#[feature]` is ambiguous with Rust's own `feature` attribute wherever it is
//! imported by name.
//!
//! ```ignore
//! #[installs]
//! impl Feature for Tabs {
//!     type Exports = (contracts::Tabs,);
//!
//!     fn install(cx: &FeatureInitContext, context: &str) -> anyhow::Result<Self> { .. }
//! }
//! ```
//!
//! `type Params = str;` beside an `install` that already takes `&str` is the
//! same fact written twice, and the two can drift: change the parameter and
//! the associated type keeps compiling until something else reads it. The
//! signature is the one that cannot be wrong, because it is what the body
//! uses, so the macro reads the type from there.
//!
//! It is not the same job as `Exports`. What a feature publishes is a
//! decision, not a consequence - a feature may claim four reducers and export
//! one - so there is nothing to derive it from, and it stays written down.

use proc_macro::TokenStream as TokenStream1;
use quote::quote;
use syn::{FnArg, ImplItem, ItemImpl, Type, parse_quote};

pub fn installs_impl(item: TokenStream1) -> TokenStream1 {
    let mut item = syn::parse_macro_input!(item as ItemImpl);

    let declares_params = item
        .items
        .iter()
        .any(|entry| matches!(entry, ImplItem::Type(ty) if ty.ident == "Params"));

    if declares_params {
        return quote!(#item).into();
    }

    let Some(params) = params_of_install(&item) else {
        // Nothing to read it off. Say so here rather than letting the missing
        // associated type be reported against a trait the author did not write.
        return syn::Error::new_spanned(
            &item,
            "#[installs] reads `Params` from `install`'s second argument, and found none - \
             write `fn install(cx: &FeatureInitContext, params: &YourParams)`, or declare \
             `type Params` yourself",
        )
        .to_compile_error()
        .into();
    };

    item.items.push(parse_quote! {
        type Params = #params;
    });

    quote!(#item).into()
}

/// The type behind `install`'s second parameter: `&str` gives `str`,
/// `&ProcessesParams` gives `ProcessesParams`.
///
/// By reference, because that is what the trait hands over - a feature is
/// given what the route captured, not ownership of it.
fn params_of_install(item: &ItemImpl) -> Option<Type> {
    let install = item.items.iter().find_map(|entry| match entry {
        ImplItem::Fn(f) if f.sig.ident == "install" => Some(f),
        _ => None,
    })?;

    let second = install.sig.inputs.iter().nth(1)?;
    let FnArg::Typed(second) = second else {
        return None;
    };

    match &*second.ty {
        Type::Reference(reference) => Some((*reference.elem).clone()),
        // Taken by value. Unusual, and the author may have meant it, so it is
        // reported by the trait's own bound rather than second-guessed here.
        other => Some(other.clone()),
    }
}

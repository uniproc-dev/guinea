//! `#[guinea::test]` - a test run once per seed, each on a fresh `Harness`.

use proc_macro::TokenStream as TokenStream1;
use proc_macro_crate::{FoundCrate, crate_name};
use proc_macro2::TokenStream;
use quote::quote;
use syn::{ItemFn, LitInt, LitStr, parse_macro_input};

const ITERATIONS: u64 = 64;

/// Where `check` and `Harness` live, from wherever the test is written.
///
/// `guinea-app` first: inside `guinea`'s own integration tests the facade is
/// the crate being tested, and `crate::` there names the test, not the facade.
fn app_path() -> TokenStream {
    let named = |name: &str| {
        let ident = syn::Ident::new(name, proc_macro2::Span::call_site());
        quote!(::#ident::app)
    };

    match crate_name("guinea-app") {
        Ok(FoundCrate::Itself) => return quote!(::guinea_app::app),
        Ok(FoundCrate::Name(name)) => return named(&name),
        Err(_) => {}
    }

    match crate_name("guinea") {
        Ok(FoundCrate::Name(name)) => named(&name),
        _ => quote!(::guinea::app),
    }
}

pub fn test_impl(attr: TokenStream1, item: TokenStream1) -> TokenStream1 {
    let mut iterations = ITERATIONS;
    let mut exclusive: Option<LitStr> = None;
    let parser = syn::meta::parser(|meta| {
        if meta.path.is_ident("iterations") {
            iterations = meta.value()?.parse::<LitInt>()?.base10_parse()?;
            Ok(())
        } else if meta.path.is_ident("exclusive") {
            exclusive = Some(meta.value()?.parse()?);
            Ok(())
        } else {
            Err(meta.error("expected `iterations = N` or `exclusive = \"key\"`"))
        }
    });
    parse_macro_input!(attr with parser);

    let mut body = parse_macro_input!(item as ItemFn);
    if body.sig.inputs.len() != 1 {
        return syn::Error::new_spanned(
            &body.sig,
            "a #[guinea::test] takes the harness and nothing else: `fn name(h: &mut Harness)`",
        )
        .to_compile_error()
        .into();
    }

    let attrs = std::mem::take(&mut body.attrs);
    let vis = &body.vis;
    let name = &body.sig.ident;
    let app = app_path();

    let run = match exclusive {
        Some(key) => quote!(#app::check_exclusive(#key, #iterations, #name);),
        None => quote!(#app::check(#iterations, #name);),
    };

    quote! {
        #(#attrs)*
        #[::core::prelude::v1::test]
        #vis fn #name() {
            #body

            #run
        }
    }
    .into()
}

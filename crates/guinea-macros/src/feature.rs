//! `#[feature_bindings]` on `impl FeatureBindings for <Name>Feature {}` -
//! fills in `type Bindings` for you, so nothing has to spell out the name
//! `generate_feature_bindings_adapter` gives the generated `<Feature>Bindings`
//! struct. The name is derived purely by convention (marker `FooFeature` ->
//! generated `FooBindings`, the same convention the codegen itself uses via
//! `pascal_case(feature)`), not by registry lookup - both sides just have to
//! agree on the same naming rule.
//!
//! Separate from `FeatureState` (State/Push/reduce) entirely: a feature
//! with no dispatch never writes this impl at all.

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemImpl, Type, parse_macro_input};

pub fn feature_impl(input: TokenStream) -> TokenStream {
    let mut impl_block = parse_macro_input!(input as ItemImpl);

    let marker_ident = match &*impl_block.self_ty {
        Type::Path(p) => p
            .path
            .segments
            .last()
            .unwrap_or_else(|| panic!("#[feature_bindings] expects `impl FeatureBindings for <Marker>`"))
            .ident
            .clone(),
        _ => panic!("#[feature_bindings] expects `impl FeatureBindings for <Marker>`"),
    };

    let marker_name = marker_ident.to_string();
    let base = marker_name.strip_suffix("Feature").unwrap_or_else(|| {
        panic!(
            "#[feature_bindings] expects the marker type to be named `<Name>Feature` (got `{marker_name}`)"
        )
    });
    let bindings_ident = quote::format_ident!("{}Bindings", base);

    impl_block
        .items
        .push(syn::parse_quote! { type Bindings = #bindings_ident; });

    quote! { #impl_block }.into()
}

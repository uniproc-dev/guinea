//! Build-time codegen: for each feature registered via `#[bindings]`,
//! generates a `<Feature>Bindings` struct - one registration slot per
//! bindings method (an actor's install code fills it), plus `emit_*` invoke
//! methods a view calls to dispatch. Resolved through
//! `guinea_core::store::Store::bindings`, so both sides share the same
//! instance. Mirrors `stub_gen`'s domain-test-kit generation but targets a
//! live `Store` instead of a message-recording stub; `emit_*` naming matches
//! `stub_gen::generate_feature_stub` on purpose (same "fire this as if the
//! UI did it" concept in both places).
//!
//! Nothing generated here for the Port side: a port trait is exactly one
//! `send(&self, msg)` method (query-style extra methods are a deprecated
//! exception this model doesn't carry forward), so `#[port]` itself emits a
//! blanket impl for any `Fn(Msg)` closure - wiring a port to `Store::push`
//! is just `move |msg| store.push::<F>(msg)` at the call site, no generated
//! adapter struct needed.
//!
//! The feature marker type (the `Store` lookup key, e.g. `ProcessesFeature`
//! for feature "processes") is expected to already exist, hand-written,
//! implementing `guinea_core::store::FeatureState` - this only generates the
//! mechanical `Bindings` side; `State`/`reduce` is business logic, not
//! derivable from a trait's method signatures.

use guinea_core::contracts::BindingStubMeta;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use crate::stub_gen::{pascal_case, qualify_ty};

pub fn generate_feature_bindings_adapter(
    feature: &str,
    bindings: &[&BindingStubMeta],
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
) -> TokenStream {
    let bindings_name = format_ident!("{}Bindings", pascal_case(feature));

    let mut state_fields = Vec::new();
    let mut invoke_methods = Vec::new();
    let mut trait_impls = Vec::new();

    for binding in bindings {
        let mut binding_methods_impl = Vec::new();

        for method in binding.methods {
            let m_name = format_ident!("{}", method.name);
            let emit_m_name = format_ident!("emit_{}", method.name);
            let handler_field = format_ident!("{}_handler", method.name);
            let arg_types: Vec<syn::Type> = method
                .arg_types
                .iter()
                .map(|ty| qualify_ty(feature, ty, contracts_crate, resolve_override))
                .collect();
            let arg_indices: Vec<syn::Ident> = (0..arg_types.len())
                .map(|i| format_ident!("arg{}", i))
                .collect();

            state_fields.push(quote! {
                #handler_field: std::cell::RefCell<Option<Box<dyn Fn(#(#arg_types),*)>>>,
            });

            invoke_methods.push(quote! {
                pub fn #emit_m_name(&self, #(#arg_indices: #arg_types),*) {
                    let handler = self.#handler_field.borrow();
                    if let Some(handler) = handler.as_ref() {
                        handler(#(#arg_indices),*);
                    }
                }
            });

            binding_methods_impl.push(quote! {
                fn #m_name<F>(&self, handler: F) where F: Fn(#(#arg_types),*) + 'static {
                    *self.#handler_field.borrow_mut() = Some(Box::new(handler));
                }
            });
        }

        let trait_path = format_ident!("{}", binding.trait_name);
        let contracts_crate_ident = format_ident!("{}", contracts_crate);
        let feature_ident = format_ident!("{}", feature);
        trait_impls.push(quote! {
            impl #contracts_crate_ident::features::#feature_ident::#trait_path for #bindings_name {
                #(#binding_methods_impl)*
            }
        });
    }

    quote! {
        #[derive(Default)]
        pub struct #bindings_name {
            #(#state_fields)*
        }

        impl #bindings_name {
            #(#invoke_methods)*
        }

        #(#trait_impls)*
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use guinea_core::contracts::BindingMethodMeta;

    #[test]
    fn generated_tokens_match_snapshot() {
        let binding = BindingStubMeta {
            trait_name: "ProcessBindings",
            feature: "processes",
            methods: &[
                BindingMethodMeta {
                    name: "on_terminate",
                    arg_types: &["u32"],
                    is_manual: false,
                    tracing_skip: false,
                    tracing_target: None,
                    slint_name: None,
                    slint_arg_types: None,
                    slint_global_override: None,
                    slint_skip: false,
                    slint_import: None,
                },
                BindingMethodMeta {
                    name: "on_sort_by",
                    arg_types: &["String"],
                    is_manual: false,
                    tracing_skip: false,
                    tracing_target: None,
                    slint_name: None,
                    slint_arg_types: None,
                    slint_global_override: None,
                    slint_skip: false,
                    slint_import: None,
                },
            ],
        };

        let tokens =
            generate_feature_bindings_adapter("processes", &[&binding], "app_contracts", &|_| {
                None
            });

        // Parsing first gives a clear "codegen produced invalid syntax"
        // failure instead of an opaque rustfmt error if something's wrong,
        // before the snapshot even gets a chance to render.
        syn::parse2::<syn::File>(tokens.clone())
            .expect("generated bindings adapter code is not valid Rust syntax");

        insta::assert_snapshot!(crate::stub_gen::format_code(tokens.to_string()));
    }
}

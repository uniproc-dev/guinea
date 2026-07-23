//! Emits `inventory::submit!` registrations (+ a small compile-time proof of
//! the `UiXPort` -> `UiXPortMsg` naming convention for ports) directly at the
//! definition site of a `#[port]`/`#[bindings]`-annotated trait - called from
//! the proc-macro while it still holds the trait's `syn::ItemTrait`, so there
//! is no source re-parsing and no JSON round-trip involved.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::ItemTrait;

/// Emits, right next to a `#[capability("key")]`-annotated struct: a `pub
/// const <KEY>: &str = "key";` (so existing `app_contracts::features::x::KEY`
/// call sites keep working with zero cross-crate lookup - this needs no
/// registry at all, the key is known at the macro's own call site) plus an
/// `inventory::submit!` registration (for consumers that need the *whole*
/// list of capabilities at once, e.g. generating `capabilities.slint`).
pub fn emit_capability_registration(key: &str, struct_name: &str) -> TokenStream {
    let const_name = format_ident!("{}", key.to_uppercase().replace(['.', '-'], "_"));

    quote! {
        pub const #const_name: &str = #key;

        ::inventory::submit! {
            guinea_core::contracts::CapabilityMeta {
                key: #key,
                struct_name: #struct_name,
            }
        }
    }
}

pub fn emit_port_registration(item_trait: &ItemTrait) -> TokenStream {
    let trait_name = item_trait.ident.to_string();
    let msg_type_name = format!("{trait_name}Msg");
    let feature = feature_from_trait_name(&trait_name);

    let msg_ident = format_ident!("{}", msg_type_name);
    let proof_fn = format_ident!("__assert_{}_shape", trait_name);

    let extra_methods = parse_port_extra_methods(item_trait);
    let extra_method_entries = extra_methods.iter().map(|m| {
        let name = &m.name;
        let arg_names = &m.arg_names;
        let arg_types = &m.arg_types;
        let output_ty = opt_str_tokens(&m.output_ty);
        quote! {
            guinea_core::contracts::PortExtraMethod {
                name: #name,
                args: &[#((#arg_names, #arg_types)),*],
                output_ty: #output_ty,
            }
        }
    });

    quote! {
        // `pub` (not just crate-private) + called once from
        // `slint-adapter`/`domain-test-kit`'s build scripts as a forced
        // link-time reference: with a single-codegen-unit override for
        // `app-contracts` (see root `Cargo.toml`), one such reference is
        // enough to keep every `inventory::submit!` in this crate from being
        // dropped by the linker for being otherwise unreferenced.
        #[doc(hidden)]
        #[allow(non_snake_case, dead_code)]
        pub fn #proof_fn(_: #msg_ident) {}

        ::inventory::submit! {
            guinea_core::contracts::PortStubMeta {
                trait_name: #trait_name,
                msg_type_name: #msg_type_name,
                feature: #feature,
                extra_methods: &[#(#extra_method_entries),*],
            }
        }
    }
}

struct PortExtraMethodInfo {
    name: String,
    arg_names: Vec<String>,
    arg_types: Vec<String>,
    output_ty: Option<String>,
}

/// Methods on a port trait other than `send` - currently only
/// `UiProcessesPort::get_selected_pid(&self) -> i32`.
///
/// TODO: see the matching TODO on `PortStubMeta::extra_methods` in
/// `guinea_core::contracts` - this whole extraction step exists solely for
/// that one non-conforming method.
fn parse_port_extra_methods(item_trait: &ItemTrait) -> Vec<PortExtraMethodInfo> {
    let mut methods = Vec::new();
    for item in &item_trait.items {
        if let syn::TraitItem::Fn(method) = item {
            let name = method.sig.ident.to_string();
            if name == "send" {
                continue;
            }
            let mut arg_names = Vec::new();
            let mut arg_types = Vec::new();
            for input in &method.sig.inputs {
                if let syn::FnArg::Typed(pat_type) = input {
                    let ty = &pat_type.ty;
                    let ty_str = quote::quote!(#ty).to_string().replace(' ', "");
                    let name_str = if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                        pat_ident.ident.to_string()
                    } else {
                        "arg".to_string()
                    };
                    arg_names.push(name_str);
                    arg_types.push(ty_str);
                }
            }
            let output_ty = match &method.sig.output {
                syn::ReturnType::Default => None,
                syn::ReturnType::Type(_, ty) => {
                    Some(quote::quote!(#ty).to_string().replace(' ', ""))
                }
            };
            methods.push(PortExtraMethodInfo {
                name,
                arg_names,
                arg_types,
                output_ty,
            });
        }
    }
    methods
}

pub fn emit_binding_registration(item_trait: &ItemTrait) -> TokenStream {
    let trait_name = item_trait.ident.to_string();
    let feature = feature_from_trait_name(&trait_name);
    let methods = parse_binding_methods(item_trait);

    let method_entries = methods.iter().map(|m| {
        let name = &m.name;
        let arg_types = &m.arg_types;
        let slint_name = opt_str_tokens(&m.slint_name);
        let slint_arg_types = match &m.slint_arg_types {
            Some(types) => quote! { Some(&[#(#types),*]) },
            None => quote! { None },
        };
        let slint_global_override = opt_str_tokens(&m.slint_global_override);
        let slint_skip = m.slint_skip;
        let slint_import = opt_str_tokens(&m.slint_import);
        let is_manual = m.is_manual;
        let tracing_skip = m.tracing_skip;
        let tracing_target = opt_str_tokens(&m.tracing_target);
        quote! {
            guinea_core::contracts::BindingMethodMeta {
                name: #name,
                arg_types: &[#(#arg_types),*],
                is_manual: #is_manual,
                tracing_skip: #tracing_skip,
                tracing_target: #tracing_target,
                slint_name: #slint_name,
                slint_arg_types: #slint_arg_types,
                slint_global_override: #slint_global_override,
                slint_skip: #slint_skip,
                slint_import: #slint_import,
            }
        }
    });

    quote! {
        ::inventory::submit! {
            guinea_core::contracts::BindingStubMeta {
                trait_name: #trait_name,
                feature: #feature,
                methods: &[#(#method_entries),*],
            }
        }
    }
}

fn opt_str_tokens(value: &Option<String>) -> TokenStream {
    match value {
        Some(v) => quote! { Some(#v) },
        None => quote! { None },
    }
}

struct BindingMethodInfo {
    name: String,
    arg_types: Vec<String>,
    is_manual: bool,
    tracing_skip: bool,
    tracing_target: Option<String>,
    slint_name: Option<String>,
    slint_arg_types: Option<Vec<String>>,
    slint_global_override: Option<String>,
    slint_skip: bool,
    slint_import: Option<String>,
}

fn parse_binding_methods(item_trait: &ItemTrait) -> Vec<BindingMethodInfo> {
    let mut methods = Vec::new();
    for item in &item_trait.items {
        if let syn::TraitItem::Fn(method) = item {
            let is_manual = has_attribute(&method.attrs, "manual");
            let tracing_skip = has_attribute(&method.attrs, "tracing")
                && has_attribute_flag(&method.attrs, "tracing", "skip");
            let tracing_target = extract_attribute_arg(&method.attrs, "tracing", "target");
            let slint_name = extract_attribute_arg(&method.attrs, "slint", "name");
            let slint_arg_types = extract_attribute_arg(&method.attrs, "slint", "arg_types")
                .map(|s| s.split(',').map(|t| t.trim().to_string()).collect());
            let slint_global_override = extract_attribute_arg(&method.attrs, "slint", "global");
            let slint_skip = has_attribute_flag(&method.attrs, "slint", "skip");
            let slint_import = extract_attribute_arg(&method.attrs, "slint", "import");

            methods.push(BindingMethodInfo {
                name: method.sig.ident.to_string(),
                arg_types: extract_handler_arg_types(method),
                is_manual,
                tracing_skip,
                tracing_target,
                slint_name,
                slint_arg_types,
                slint_global_override,
                slint_skip,
                slint_import,
            });
        }
    }
    methods
}

fn has_attribute(attrs: &[syn::Attribute], attr_name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(attr_name))
}

fn has_attribute_flag(attrs: &[syn::Attribute], attr_name: &str, flag_name: &str) -> bool {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            let mut found = false;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident(flag_name) {
                    found = true;
                }
                Ok(())
            });
            if found {
                return true;
            }
        }
    }
    false
}

fn extract_attribute_arg(attrs: &[syn::Attribute], attr_name: &str, key: &str) -> Option<String> {
    for attr in attrs {
        if attr.path().is_ident(attr_name) {
            let mut value = None;
            let _ = attr.parse_nested_meta(|meta| {
                if meta.path.is_ident(key) {
                    if let Ok(v) = meta.value() {
                        if let Ok(s) = v.parse::<syn::LitStr>() {
                            value = Some(s.value());
                        }
                    }
                }
                Ok(())
            });
            if value.is_some() {
                return value;
            }
        }
    }
    None
}

fn extract_handler_arg_types(method: &syn::TraitItemFn) -> Vec<String> {
    let Some(where_clause) = &method.sig.generics.where_clause else {
        return Vec::new();
    };

    for predicate in &where_clause.predicates {
        let syn::WherePredicate::Type(pred) = predicate else {
            continue;
        };
        let syn::Type::Path(type_path) = &pred.bounded_ty else {
            continue;
        };
        if type_path
            .path
            .segments
            .last()
            .map(|s| s.ident != "F")
            .unwrap_or(true)
        {
            continue;
        }
        for bound in &pred.bounds {
            let syn::TypeParamBound::Trait(trait_bound) = bound else {
                continue;
            };
            let Some(segment) = trait_bound.path.segments.last() else {
                continue;
            };
            if segment.ident != "Fn" && segment.ident != "FnMut" && segment.ident != "FnOnce" {
                continue;
            }
            let syn::PathArguments::Parenthesized(args) = &segment.arguments else {
                continue;
            };
            return args
                .inputs
                .iter()
                .map(|ty| quote::quote!(#ty).to_string().replace(' ', ""))
                .collect();
        }
    }

    Vec::new()
}

fn feature_from_trait_name(trait_name: &str) -> String {
    // A handful of traits don't share their feature's plain plural name
    // (e.g. `UiServiceDetailsPort` lives in the `services` feature alongside
    // `UiServicesPort`/`UiServicesBindings`, not a separate `service_details`
    // one) - the generic Ui/Port/Bindings stripping below can't know that on
    // its own, so it's a short, explicit exception list instead.
    if let Some(feature) = KNOWN_FEATURE_OVERRIDES
        .iter()
        .find_map(|(name, feature)| (*name == trait_name).then_some(*feature))
    {
        return feature.to_string();
    }

    let stripped = trait_name.strip_prefix("Ui").unwrap_or(trait_name);
    let stripped = stripped
        .strip_suffix("Bindings")
        .or_else(|| stripped.strip_suffix("Port"))
        .unwrap_or(stripped);
    to_snake_case(stripped)
}

const KNOWN_FEATURE_OVERRIDES: &[(&str, &str)] = &[("UiServiceDetailsPort", "services")];

pub(crate) fn to_snake_case(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.char_indices() {
        if c.is_uppercase() {
            if i != 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

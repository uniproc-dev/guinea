use proc_macro::TokenStream;
use proc_macro2::Ident;
use quote::quote;
use syn::{FnArg, ImplItem, ImplItemFn, ItemImpl, Pat, parse_quote};

const HELPER_ATTRS: &[&str] = &["manual", "tracing", "slint"];

fn strip_helper_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| {
        !HELPER_ATTRS
            .iter()
            .any(|helper| attr.path().is_ident(helper))
    });
}

/// `#[port_adapter(backend = "slint", window = AppWindow)]` - ports only ever
/// have hand-written `send()` bodies now, so there is nothing left to
/// auto-generate here; this just applies the upgrade-check/tracing wrap.
/// `backend` is accepted (and currently only `"slint"` is implemented) so
/// adding a second UI backend later means a new `match` arm, not a new macro.
pub fn port_adapter_impl(attr: TokenStream, mut impl_block: ItemImpl) -> TokenStream {
    require_slint_backend(&attr);
    apply_adapter_transform(&mut impl_block);
    quote!(#impl_block).into()
}

/// `#[bindings_adapter(backend = "slint", window = AppWindow)]` - only wraps
/// whatever methods are hand-written (`#[manual]`) in this impl block with
/// the upgrade-check/tracing transform. Non-manual methods are generated
/// wholesale as a *separate* `impl` block by the consuming app's own
/// build.rs (which can read the full registry via a build-dependency,
/// something this proc-macro fundamentally cannot do) and `include!`-d
/// alongside this one - they never pass through this macro at all.
pub fn bindings_adapter_impl(attr: TokenStream, mut impl_block: ItemImpl) -> TokenStream {
    require_slint_backend(&attr);
    apply_adapter_transform(&mut impl_block);
    quote!(#impl_block).into()
}

fn require_slint_backend(attr: &TokenStream) {
    let backend = extract_attr_arg(attr, "backend").unwrap_or_else(|| "slint".to_string());
    if backend != "slint" {
        panic!("unsupported backend {backend:?} (only \"slint\" is implemented)");
    }
}

fn extract_attr_arg(attr: &TokenStream, key: &str) -> Option<String> {
    let s = attr.to_string();
    for part in s.split(',') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(key) {
            let rest = rest.trim_start();
            if let Some(value) = rest.strip_prefix('=') {
                return Some(value.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

pub fn get_trait_name(impl_block: &ItemImpl) -> String {
    match &impl_block.trait_ {
        Some((_, path, _)) => path.segments.last().unwrap().ident.to_string(),
        None => panic!("This macro can only be applied to trait implementations"),
    }
}

/// Wraps every method still present in this impl block (i.e. the hand-written
/// `#[manual]` ones - non-manual binding methods are generated separately by
/// the app's own build.rs and never reach this macro) with the
/// upgrade-check/tracing transform. Tracing config for a binding-style method
/// (one with a `where F: Fn(...)` handler bound) comes from `#[tracing(...)]`
/// declared directly on *this* impl method - the proc-macro has no way to
/// cross-crate-read the trait's own copy of that attribute where the trait is
/// defined, so the adapter carries its own copy for whichever methods it
/// hand-implements.
pub fn apply_adapter_transform(impl_block: &mut ItemImpl) {
    let self_ty = (*impl_block.self_ty).clone();
    let trait_name = get_trait_name(impl_block);

    for item in &mut impl_block.items {
        if let ImplItem::Fn(method) = item {
            let tracing_skip = has_attribute_flag(&method.attrs, "tracing", "skip");
            let tracing_target = extract_attribute_arg(&method.attrs, "tracing", "target");
            let handler_arity = extract_handler_arity(method);
            strip_helper_attrs(&mut method.attrs);

            let tracing = handler_arity.map(|arity| BindingTracingSpec {
                scope: build_binding_scope(&trait_name, &method.sig.ident.to_string()),
                target: tracing_target,
                enabled: !tracing_skip,
                handler_arity: arity,
            });
            transform_method(&self_ty, method, tracing.as_ref());
        }
    }
}

fn extract_handler_arity(method: &ImplItemFn) -> Option<usize> {
    let where_clause = method.sig.generics.where_clause.as_ref()?;
    for predicate in &where_clause.predicates {
        let syn::WherePredicate::Type(pred) = predicate else {
            continue;
        };
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
            if let syn::PathArguments::Parenthesized(args) = &segment.arguments {
                return Some(args.inputs.len());
            }
        }
    }
    None
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

fn transform_method(
    self_ty: &syn::Type,
    method: &mut ImplItemFn,
    binding_tracing: Option<&BindingTracingSpec>,
) {
    let ui_arg_idx = find_ui_arg_index(method);

    if let Some(idx) = ui_arg_idx {
        remove_ui_arg(method, idx);
        let handler_wrap = binding_tracing
            .filter(|spec| spec.enabled)
            .map(|spec| build_binding_tracing_wrapper(method, spec));
        let ui_port_wrap = binding_tracing
            .is_none()
            .then(|| build_ui_port_wrapper(self_ty, method));
        let ui_upgrade_failure = build_ui_upgrade_failure(self_ty, method);
        let block = &method.block;

        method.block = parse_quote!({
            let Some(ui) = self.ui.upgrade() else { #ui_upgrade_failure };
            #handler_wrap
            #ui_port_wrap
            #block
        });
    } else if let Some(spec) = binding_tracing.filter(|spec| spec.enabled) {
        let handler_wrap = build_binding_tracing_wrapper(method, spec);
        let block = &method.block;
        method.block = parse_quote!({
            #handler_wrap
            #block
        });
    }
}

fn find_ui_arg_index(method: &ImplItemFn) -> Option<usize> {
    method.sig.inputs.iter().enumerate().find_map(|(i, arg)| {
        if let FnArg::Typed(pat_type) = arg {
            if let Pat::Ident(ref id) = *pat_type.pat {
                if id.ident == "ui" {
                    return Some(i);
                }
            }
        }
        None
    })
}

fn remove_ui_arg(method: &mut ImplItemFn, idx: usize) {
    let mut inputs = syn::punctuated::Punctuated::<syn::FnArg, syn::token::Comma>::new();
    for (i, arg) in method.sig.inputs.clone().into_iter().enumerate() {
        if i != idx {
            inputs.push(arg);
        }
    }
    method.sig.inputs = inputs;
}

struct BindingTracingSpec {
    scope: String,
    target: Option<String>,
    enabled: bool,
    handler_arity: usize,
}

fn build_binding_tracing_wrapper(
    method: &ImplItemFn,
    spec: &BindingTracingSpec,
) -> proc_macro2::TokenStream {
    let handler_ident = find_handler_ident(method)
        .unwrap_or_else(|| panic!("binding tracing requires a handler parameter"));
    let scope = &spec.scope;
    let target_fields = spec
        .target
        .as_ref()
        .map(|v| quote! { Some(#v) })
        .unwrap_or_else(|| quote! { None });

    let arity = spec.handler_arity;

    match arity {
        0 => quote! {
            let handler = {
                let handler = #handler_ident;
                move || {
                    guinea_core::trace::in_ui_action_scope(#scope, #target_fields, None, || handler())
                }
            };
        },
        1 => quote! {
            let handler = {
                let handler = #handler_ident;
                move |__ui_arg0| {
                    let __ui_target = guinea_core::trace::format_ui_target_1(&__ui_arg0);
                    guinea_core::trace::in_ui_action_scope(
                        #scope,
                        #target_fields,
                        __ui_target,
                        || handler(__ui_arg0),
                    )
                }
            };
        },
        2 => quote! {
            let handler = {
                let handler = #handler_ident;
                move |__ui_arg0, __ui_arg1| {
                    let __ui_target = guinea_core::trace::format_ui_target_2(&__ui_arg0, &__ui_arg1);
                    guinea_core::trace::in_ui_action_scope(
                        #scope,
                        #target_fields,
                        __ui_target,
                        || handler(__ui_arg0, __ui_arg1),
                    )
                }
            };
        },
        _ => panic!("binding tracing currently supports handlers with up to 2 arguments"),
    }
}

fn build_binding_scope(trait_name: &str, method_name: &str) -> String {
    format!("Ui.{}.{}", binding_feature_name(trait_name), method_name)
}

fn binding_feature_name(trait_name: &str) -> String {
    let trimmed = trait_name.strip_suffix("Bindings").unwrap_or(trait_name);
    let trimmed = trimmed.strip_prefix("Ui").unwrap_or(trimmed);
    trimmed.to_string()
}

fn build_ui_port_wrapper(self_ty: &syn::Type, method: &ImplItemFn) -> proc_macro2::TokenStream {
    let method_name = method.sig.ident.to_string();
    let adapter_name = quote! { stringify!(#self_ty) };

    quote! {
        if guinea_core::trace::is_scope_enabled("ui.adapter.call") {
            let __ui_port_target_value = format!("{}::{}", #adapter_name, #method_name);
            let __ui_port_scope_target = Some(__ui_port_target_value.clone());
            let __ui_port_call = || {
                tracing::debug!(
                    adapter = #adapter_name,
                    method = #method_name,
                    "ui.adapter.call"
                );
            };
            if guinea_core::trace::is_target_enabled(&__ui_port_target_value) {
                guinea_core::trace::in_named_scope(
                    "ui.adapter.call",
                    Some("adapter,method"),
                    __ui_port_scope_target,
                    __ui_port_call,
                );
            }
        }
    }
}

fn build_ui_upgrade_failure(self_ty: &syn::Type, method: &ImplItemFn) -> proc_macro2::TokenStream {
    let method_name = method.sig.ident.to_string();
    let adapter_name = quote! { stringify!(#self_ty) };

    quote! {
        let __ui_port_target_value = format!("{}::{}", #adapter_name, #method_name);
        let __ui_port_scope_target = Some(__ui_port_target_value.clone());

        if guinea_core::trace::is_scope_enabled("ui.adapter.call")
            && guinea_core::trace::is_target_enabled(&__ui_port_target_value)
        {
            guinea_core::trace::in_named_scope(
                "ui.adapter.call",
                Some("adapter,method"),
                __ui_port_scope_target,
                || {
                    tracing::error!(
                        adapter = #adapter_name,
                        method = #method_name,
                        "ui.adapter.upgrade_failed"
                    );
                },
            );
        }

        panic!("ui handle is dropped in {}::{}", #adapter_name, #method_name);
    }
}

fn find_handler_ident(method: &ImplItemFn) -> Option<Ident> {
    method.sig.inputs.iter().find_map(|arg| {
        if let FnArg::Typed(pat_type) = arg
            && let Pat::Ident(pat_ident) = pat_type.pat.as_ref()
            && pat_ident.ident == "handler"
        {
            Some(pat_ident.ident.clone())
        } else {
            None
        }
    })
}

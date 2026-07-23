//! Full test-double codegen engine driven by the `guinea_core::contracts`
//! registry: for each feature, a message-recording stub struct implementing
//! every `#[port]`/`#[bindings]` trait registered for it. Callers own
//! grouping-by-feature, file writing, and the app-specific type-resolution
//! override (anything the registry's string type name doesn't resolve to
//! `<contracts_crate>::features::<feature>::<Ident>` by the default
//! convention).

use guinea_core::contracts::{BindingMethodMeta, BindingStubMeta, PortExtraMethod, PortStubMeta};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{GenericArgument, Ident, PathArguments, Type, parse_str};

/// Generates one `<Feature>UiStub` struct implementing every port/bindings
/// trait registered for `feature`.
///
/// `contracts_crate` is the crate path where the `#[port]`/`#[bindings]`
/// traits themselves live (e.g. `"app_contracts"`) - used for the default
/// `<contracts_crate>::features::<feature>::<Ident>` type-resolution
/// fallback. `resolve_override` lets the caller supply app-specific
/// exceptions to that fallback (e.g. a shared type that actually lives
/// somewhere other than the contracts crate) - return `None` to fall
/// through to the default.
pub fn generate_feature_stub(
    feature: &str,
    ports: &[&PortStubMeta],
    bindings: &[&BindingStubMeta],
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
) -> TokenStream {
    let struct_name = format_ident!("{}UiStub", pascal_case(feature));
    let state_name = format_ident!("{}State", struct_name);

    let mut state_fields = Vec::new();
    let mut stub_impl_methods = Vec::new();
    let mut trait_impls = Vec::new();

    let interaction_path = quote! { guinea_core::test_kit::Interaction };

    for port in ports {
        let port_prefix = snake_case(port.trait_name);
        let msg_ty = qualify_ty(feature, port.msg_type_name, contracts_crate, resolve_override);
        let sent_field = format_ident!("{}_sent", port_prefix);

        state_fields.push(quote! { #sent_field: std::cell::RefCell<Vec<#msg_ty>>, });
        stub_impl_methods.push(quote! {
            pub fn #sent_field(&self) -> #interaction_path<Vec<#msg_ty>> {
                #interaction_path::new(self.inner.#sent_field.borrow().clone())
            }
        });

        let mut port_methods_impl = vec![quote! {
            fn send(&self, msg: #msg_ty) {
                self.inner.#sent_field.borrow_mut().push(msg);
            }
        }];

        for method in port.extra_methods {
            port_methods_impl.push(generate_port_extra_method(
                feature,
                &struct_name,
                &port_prefix,
                method,
                contracts_crate,
                resolve_override,
                &mut state_fields,
                &mut stub_impl_methods,
            ));
        }

        let trait_path = format_ident!("{}", port.trait_name);
        let contracts_crate_ident = format_ident!("{}", contracts_crate);
        let feature_ident = format_ident!("{}", feature);
        trait_impls.push(quote! {
            impl #contracts_crate_ident::features::#feature_ident::#trait_path for #struct_name {
                #(#port_methods_impl)*
            }
        });
    }

    for binding in bindings {
        let mut binding_methods_impl = Vec::new();

        for method in binding.methods {
            let m_name = format_ident!("{}", method.name);
            let emit_m_name = format_ident!("emit_{}", method.name);
            let handler_field = format_ident!("{}_handler", method.name);
            let arg_types: Vec<Type> = method
                .arg_types
                .iter()
                .map(|ty| qualify_ty(feature, ty, contracts_crate, resolve_override))
                .collect();
            let arg_indices: Vec<Ident> = (0..arg_types.len())
                .map(|i| format_ident!("arg{}", i))
                .collect();

            state_fields.push(
                quote! { #handler_field: std::cell::RefCell<Option<Box<dyn Fn(#(#arg_types),*)>>>, },
            );

            stub_impl_methods.push(quote! {
                pub fn #emit_m_name(&self, #(#arg_indices: #arg_types),*) -> #interaction_path<()> {
                    let handler = self.inner.#handler_field.borrow();
                    let handler = handler.as_ref().expect(concat!(stringify!(#struct_name), "::", stringify!(#m_name), " handler not registered"));
                    handler(#(#arg_indices),*);
                    #interaction_path::new(())
                }
            });

            binding_methods_impl.push(quote! {
                fn #m_name<F>(&self, handler: F) where F: Fn(#(#arg_types),*) + 'static {
                    *self.inner.#handler_field.borrow_mut() = Some(Box::new(handler));
                }
            });
        }

        let trait_path = format_ident!("{}", binding.trait_name);
        let contracts_crate_ident = format_ident!("{}", contracts_crate);
        let feature_ident = format_ident!("{}", feature);
        trait_impls.push(quote! {
            impl #contracts_crate_ident::features::#feature_ident::#trait_path for #struct_name {
                #(#binding_methods_impl)*
            }
        });
    }

    quote! {
        #[derive(Default)]
        struct #state_name {
            #(#state_fields)*
        }

        #[derive(Clone, Default)]
        pub struct #struct_name {
            inner: std::rc::Rc<#state_name>,
        }

        impl std::fmt::Debug for #struct_name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.debug_struct(stringify!(#struct_name)).finish()
            }
        }

        impl #struct_name {
            pub fn new() -> Self { Self::default() }
            #(#stub_impl_methods)*
        }

        #(#trait_impls)*
    }
}

/// A port method other than `send` (currently only
/// `UiProcessesPort::get_selected_pid`) - modeled the same way binding
/// handlers are: a settable closure the test installs, plus a call counter.
///
/// TODO: this only exists for `get_selected_pid`, the sole port method that
/// isn't `send(msg)` - see the matching TODO on `PortStubMeta::extra_methods`
/// in `guinea_core::contracts`. Once that's reworked to be message-based,
/// this function (and the whole `extra_methods` codepath) can go away.
#[allow(clippy::too_many_arguments)]
fn generate_port_extra_method(
    feature: &str,
    struct_name: &proc_macro2::Ident,
    port_prefix: &str,
    method: &PortExtraMethod,
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
    state_fields: &mut Vec<TokenStream>,
    stub_impl_methods: &mut Vec<TokenStream>,
) -> TokenStream {
    let m_name = format_ident!("{}", method.name);
    let on_m_name = format_ident!("on_{}_{}", port_prefix, method.name);
    let count_field = format_ident!("{}_{}_call_count", port_prefix, method.name);
    let handler_field = format_ident!("{}_{}_handler", port_prefix, method.name);

    state_fields.push(quote! { #count_field: std::cell::RefCell<usize>, });
    stub_impl_methods.push(quote! {
        pub fn #count_field(&self) -> guinea_core::test_kit::Interaction<usize> {
            guinea_core::test_kit::Interaction::new(*self.inner.#count_field.borrow())
        }
    });

    let arg_names: Vec<Ident> = method
        .args
        .iter()
        .map(|(name, _)| format_ident!("{}", name))
        .collect();
    let arg_types: Vec<Type> = method
        .args
        .iter()
        .map(|(_, ty)| qualify_ty(feature, ty, contracts_crate, resolve_override))
        .collect();

    if let Some(out_ty_str) = method.output_ty {
        let out_ty = qualify_ty(feature, out_ty_str, contracts_crate, resolve_override);
        state_fields.push(quote! { #handler_field: std::cell::RefCell<Option<Box<dyn Fn(#(#arg_types),*) -> #out_ty>>>, });
        stub_impl_methods.push(quote! {
            pub fn #on_m_name<F>(&self, handler: F)
            where F: Fn(#(#arg_types),*) -> #out_ty + 'static
            { *self.inner.#handler_field.borrow_mut() = Some(Box::new(handler)); }
        });

        quote! {
            fn #m_name(&self, #(#arg_names: #arg_types),*) -> #out_ty {
                *self.inner.#count_field.borrow_mut() += 1;
                let handler = self.inner.#handler_field.borrow();
                let handler = handler.as_ref().expect(concat!(stringify!(#struct_name), "::", stringify!(#m_name), " requires a handler"));
                handler(#(#arg_names),*)
            }
        }
    } else {
        quote! {
            fn #m_name(&self, #(#arg_names: #arg_types),*) {
                *self.inner.#count_field.borrow_mut() += 1;
            }
        }
    }
}

/// Formats generated code via `rustfmt`, falling back to the unformatted
/// string if `rustfmt` isn't on `PATH` (output still compiles either way -
/// this is purely for readability when inspecting the generated file).
pub fn format_code(code: String) -> String {
    use std::io::Write;
    use std::process::{Command, Stdio};

    let child = Command::new("rustfmt")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();

    match child {
        Ok(mut child) => {
            let mut stdin = child.stdin.take().expect("Failed to open stdin");
            let _ = stdin.write_all(code.as_bytes());
            drop(stdin);

            let output = child
                .wait_with_output()
                .expect("Failed to read rustfmt output");
            if output.status.success() {
                String::from_utf8(output.stdout).expect("Invalid UTF-8 from rustfmt")
            } else {
                code
            }
        }
        Err(_) => code,
    }
}

fn qualify_ty(
    feature: &str,
    ty_str: &str,
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
) -> Type {
    let ty: Type = parse_str(ty_str).unwrap_or_else(|_| panic!("failed to parse type: {}", ty_str));
    rewrite_type(feature, ty, contracts_crate, resolve_override)
}

fn rewrite_type(
    feature: &str,
    ty: Type,
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
) -> Type {
    match ty {
        Type::Path(mut path) if path.qself.is_none() => {
            let first_segment = &path.path.segments[0];
            let ident = first_segment.ident.to_string();
            let original_args = first_segment.arguments.clone();

            if is_std_type(&ident) {
                if let PathArguments::AngleBracketed(args) = &mut path.path.segments[0].arguments {
                    for arg in &mut args.args {
                        if let GenericArgument::Type(inner) = arg {
                            *inner = rewrite_type(feature, inner.clone(), contracts_crate, resolve_override);
                        }
                    }
                }
                return Type::Path(path);
            }

            let mut new_path = resolved_path_for(feature, &ident, contracts_crate, resolve_override);
            if let Some(last) = new_path.segments.last_mut() {
                last.arguments = original_args;
                if let PathArguments::AngleBracketed(args) = &mut last.arguments {
                    for arg in &mut args.args {
                        if let GenericArgument::Type(inner) = arg {
                            *inner = rewrite_type(feature, inner.clone(), contracts_crate, resolve_override);
                        }
                    }
                }
            }
            Type::Path(syn::TypePath {
                qself: None,
                path: new_path,
            })
        }
        Type::Reference(mut r) => {
            *r.elem = rewrite_type(feature, *r.elem, contracts_crate, resolve_override);
            Type::Reference(r)
        }
        Type::Slice(mut s) => {
            *s.elem = rewrite_type(feature, *s.elem, contracts_crate, resolve_override);
            Type::Slice(s)
        }
        other => other,
    }
}

fn is_std_type(ident: &str) -> bool {
    matches!(
        ident,
        "bool"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f32"
            | "f64"
            | "String"
            | "str"
            | "Vec"
            | "Option"
            | "Box"
            | "Result"
    )
}

fn resolved_path_for(
    feature: &str,
    ident: &str,
    contracts_crate: &str,
    resolve_override: &dyn Fn(&str) -> Option<syn::Path>,
) -> syn::Path {
    if ident == "SharedString" {
        return parse_str("slint::SharedString").unwrap();
    }
    if let Some(path) = resolve_override(ident) {
        return path;
    }
    parse_str(&format!("{contracts_crate}::features::{feature}::{ident}")).unwrap()
}

fn snake_case(name: &str) -> String {
    crate::contracts::to_snake_case(name)
}

fn pascal_case(name: &str) -> String {
    name.split('_')
        .map(|s| {
            let mut c = s.chars();
            match c.next() {
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

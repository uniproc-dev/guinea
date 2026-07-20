//! Slint-backend codegen engine driven purely by the `forsl_core::contracts`
//! registry: `.slint` global text and adapter `impl` bodies. Callers own all
//! file I/O and path layout (that's app-specific); this module only turns
//! registry entries into generated source text.

use forsl_core::contracts::{BindingMethodMeta, BindingStubMeta, CapabilityMeta};

/// `.slint` global declaring one `out property <string>` per capability key.
pub fn generate_capabilities_slint_content<'a>(
    caps: impl Iterator<Item = &'a CapabilityMeta>,
) -> String {
    let mut caps: Vec<_> = caps.collect();
    caps.sort_by_key(|cap| cap.key);

    let properties = caps
        .iter()
        .map(|cap| {
            let slint_name = cap.key.replace(['.', '_'], "-");
            format!("    out property <string> {}: \"{}\";", slint_name, cap.key)
        })
        .collect::<Vec<_>>()
        .join("\n");

    format!("// AUTO-GENERATED — do not edit manually\nexport global Capabilities {{\n{properties}\n}}\n")
}

pub fn global_name_for(binding: &BindingStubMeta) -> String {
    binding
        .trait_name
        .strip_prefix("Ui")
        .unwrap_or(binding.trait_name)
        .to_string()
}

pub fn adapter_type_for(binding: &BindingStubMeta) -> String {
    binding.trait_name.replace("Bindings", "Adapter")
}

fn default_slint_type(rust_ty: &str) -> String {
    match rust_ty {
        "u8" | "u16" | "u32" | "u64" | "u128" | "usize" | "i8" | "i16" | "i32" | "i64" | "i128"
        | "isize" => "int".to_string(),
        "f32" | "f64" => "float".to_string(),
        "bool" => "bool".to_string(),
        "String" | "SharedString" => "string".to_string(),
        other => other.to_string(),
    }
}

/// `.slint` global whose callbacks mirror a `#[bindings]` trait's methods -
/// no hand-authored `.slint` needed for the common case.
pub fn generate_binding_global_slint_content(binding: &BindingStubMeta) -> String {
    let global_name = global_name_for(binding);

    let included_methods: Vec<&BindingMethodMeta> = binding
        .methods
        .iter()
        .filter(|m| !m.slint_skip && m.slint_global_override.is_none())
        .collect();

    let mut imports: Vec<&str> = included_methods
        .iter()
        .filter_map(|m| m.slint_import)
        .collect();
    imports.sort_unstable();
    imports.dedup();
    let imports = imports.join("\n");

    let callbacks = included_methods
        .iter()
        .map(|method| {
            let slint_name = method
                .slint_name
                .map(str::to_string)
                .unwrap_or_else(|| method.name.strip_prefix("on_").unwrap_or(method.name).to_string());
            let arg_types = method
                .arg_types
                .iter()
                .enumerate()
                .map(|(i, ty)| {
                    method
                        .slint_arg_types
                        .and_then(|overrides| overrides.get(i))
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| default_slint_type(ty))
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("    callback {slint_name}({arg_types});")
        })
        .collect::<Vec<_>>()
        .join("\n");

    let imports_block = if imports.is_empty() {
        String::new()
    } else {
        format!("{imports}\n\n")
    };

    format!(
        "// AUTO-GENERATED from {} - do not edit manually\n{imports_block}export global {global_name} {{\n{callbacks}\n}}\n",
        binding.trait_name,
    )
}

/// Generates the *entire* `impl <Trait> for <Adapter>` block - every method,
/// manual or not. Rust doesn't allow splitting one trait's `impl` across two
/// blocks for the same type (nor does it allow `include!` to expand inside an
/// `impl`'s braces at all - only at module level), so there's no way to keep
/// a hand-written partial impl alongside a generated one. Instead, hand-written
/// files must provide `#[manual]` bodies as plain inherent methods named
/// `<method>_manual`, and this generates a real top-level `impl` that either
/// writes the full `ui.global::<...>()` body (non-manual) or delegates to
/// `self.<name>_manual(...)` (manual) - both wrapped in the same
/// upgrade-check/tracing scaffolding.
///
/// `contracts_crate` is the crate path where the `#[bindings]` trait lives
/// (e.g. `"app_contracts"`) and `adapter_path` is where the adapter type
/// itself is expected to be found (e.g. `"crate::features"`, following the
/// `<adapter_path>::{feature}::{adapter_ty}` convention).
pub fn generate_binding_adapter_impl_content(
    binding: &BindingStubMeta,
    contracts_crate: &str,
    adapter_path: &str,
) -> String {
    let adapter_ty = adapter_type_for(binding);
    let global_name = global_name_for(binding);

    let methods = binding
        .methods
        .iter()
        .map(|m| generate_method(&adapter_ty, &global_name, m))
        .collect::<Vec<_>>()
        .join("\n\n");

    format!(
        "// AUTO-GENERATED from {trait_name} - do not edit manually\nimpl {contracts_crate}::features::{feature}::{trait_name} for {adapter_path}::{feature}::{adapter_ty} {{\n{methods}\n}}\n",
        trait_name = binding.trait_name,
        feature = binding.feature,
    )
}

fn generate_method(adapter_ty: &str, global_name: &str, method: &BindingMethodMeta) -> String {
    let name = method.name;
    let handler_types = method
        .arg_types
        .iter()
        .map(|ty| qualify_known_type_str(ty))
        .collect::<Vec<_>>()
        .join(", ");

    let upgrade_failure = ui_upgrade_failure_body(adapter_ty, name);
    let handler_wrap = binding_tracing_wrapper(method, name);

    let call = if method.is_manual {
        format!("        self.{name}_manual(&ui, handler);")
    } else {
        // Unlike the `.slint` global's callback declaration (which strips
        // `on_` - that's Slint's own naming convention for a callback vs.
        // its Rust subscription method), the *Rust-generated* subscription
        // method is always named `on_<callback>` - i.e. exactly the trait
        // method name, unless overridden.
        let slint_name =
            method.slint_name.map(str::to_string).unwrap_or_else(|| name.to_string());
        let global = method.slint_global_override.unwrap_or(global_name);
        let arg_idents: Vec<String> =
            (0..method.arg_types.len()).map(|i| format!("__arg{i}")).collect();
        let call_args = method
            .arg_types
            .iter()
            .enumerate()
            .map(|(i, ty)| convert_expr_from_slint(&arg_idents[i], ty))
            .collect::<Vec<_>>()
            .join(", ");
        let arg_idents_pattern = arg_idents.join(", ");
        format!(
            r#"        use slint::ComponentHandle;
        ui.global::<crate::{global}>().{slint_name}(move |{arg_idents_pattern}| {{
            handler({call_args});
        }});"#
        )
    };

    format!(
        r#"    fn {name}<F>(&self, handler: F)
    where F: Fn({handler_types}) + 'static
    {{
        let Some(ui) = self.ui.upgrade() else {{ {upgrade_failure} }};
{handler_wrap}
{call}
    }}"#
    )
}

fn binding_tracing_wrapper(method: &BindingMethodMeta, method_name: &str) -> String {
    if method.tracing_skip {
        return String::new();
    }
    let arity = method.arg_types.len();
    let scope = format!("Ui.{}", method_name);
    let target_expr = match method.tracing_target {
        Some(t) => format!("Some({t:?})"),
        None => "None".to_string(),
    };

    match arity {
        0 => format!(
            r#"        let handler = {{
            let handler = handler;
            move || forsl_core::trace::in_ui_action_scope({scope:?}, {target_expr}, None, || handler())
        }};"#
        ),
        1 => format!(
            r#"        let handler = {{
            let handler = handler;
            move |__ui_arg0| {{
                let __ui_target = forsl_core::trace::format_ui_target_1(&__ui_arg0);
                forsl_core::trace::in_ui_action_scope({scope:?}, {target_expr}, __ui_target, || handler(__ui_arg0))
            }}
        }};"#
        ),
        2 => format!(
            r#"        let handler = {{
            let handler = handler;
            move |__ui_arg0, __ui_arg1| {{
                let __ui_target = forsl_core::trace::format_ui_target_2(&__ui_arg0, &__ui_arg1);
                forsl_core::trace::in_ui_action_scope({scope:?}, {target_expr}, __ui_target, || handler(__ui_arg0, __ui_arg1))
            }}
        }};"#
        ),
        _ => panic!("binding tracing currently supports handlers with up to 2 arguments"),
    }
}

fn ui_upgrade_failure_body(adapter_ty: &str, method_name: &str) -> String {
    format!(
        r#"
            if forsl_core::trace::is_scope_enabled("ui.adapter.call") {{
                let __t = format!("{{}}::{{}}", {adapter_ty:?}, {method_name:?});
                if forsl_core::trace::is_target_enabled(&__t) {{
                    forsl_core::trace::in_named_scope("ui.adapter.call", Some("adapter,method"), Some(__t), || {{
                        tracing::error!(adapter = {adapter_ty:?}, method = {method_name:?}, "ui.adapter.upgrade_failed");
                    }});
                }}
            }}
            panic!("ui handle is dropped in {adapter_ty}::{method_name}");
        "#
    )
}

fn qualify_known_type_str(ty: &str) -> String {
    match ty {
        "SharedString" => "slint::SharedString".to_string(),
        "Image" => "slint::Image".to_string(),
        "Color" => "slint::Color".to_string(),
        _ => ty.to_string(),
    }
}

fn is_trivial_numeric(ty: &str) -> bool {
    matches!(
        ty,
        "u8" | "u16"
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
    )
}

fn convert_expr_from_slint(name: &str, ty: &str) -> String {
    if is_trivial_numeric(ty) {
        format!("{name} as _")
    } else if ty == "bool" {
        name.to_string()
    } else {
        format!("{name}.into()")
    }
}

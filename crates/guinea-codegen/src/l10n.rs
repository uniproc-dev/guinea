use fluent_syntax::ast::{Entry, Expression, InlineExpression, Pattern, PatternElement};
use fluent_syntax::parser::parse;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::fs;
use std::path::Path;
use syn::Path as SynPath;

#[derive(Debug, Clone, PartialEq)]
pub struct L10nMessage {
    pub id: String,
    pub variables: Vec<String>,
}

pub fn parse_messages(ftl_path: &Path) -> Vec<L10nMessage> {
    let content = fs::read_to_string(ftl_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", ftl_path.display()));
    let resource = parse(content.as_str())
        .unwrap_or_else(|(_, errors)| panic!("failed to parse {}: {errors:?}", ftl_path.display()));

    let mut messages: Vec<L10nMessage> = resource
        .body
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Message(msg) => {
                let id = msg.id.name.to_string();
                let mut variables = Vec::new();
                if let Some(pattern) = &msg.value {
                    collect_variables(pattern, &mut variables);
                }
                Some(L10nMessage { id, variables })
            }
            _ => None,
        })
        .collect();
    messages.sort_by(|a, b| a.id.cmp(&b.id));
    messages
}

fn collect_variables(pattern: &Pattern<&str>, out: &mut Vec<String>) {
    for element in &pattern.elements {
        if let PatternElement::Placeable { expression } = element {
            collect_from_expression(expression, out);
        }
    }
}

fn collect_from_expression(expr: &Expression<&str>, out: &mut Vec<String>) {
    match expr {
        Expression::Inline(inline) => collect_from_inline(inline, out),
        Expression::Select { selector, variants } => {
            collect_from_inline(selector, out);
            for variant in variants {
                collect_variables(&variant.value, out);
            }
        }
    }
}

fn collect_from_inline(inline: &InlineExpression<&str>, out: &mut Vec<String>) {
    match inline {
        InlineExpression::VariableReference { id } => push_unique(out, id.name),
        InlineExpression::FunctionReference { arguments, .. } => collect_call_args(arguments, out),
        InlineExpression::TermReference { arguments, .. } => {
            if let Some(arguments) = arguments {
                collect_call_args(arguments, out);
            }
        }
        InlineExpression::Placeable { expression } => collect_from_expression(expression, out),
        InlineExpression::StringLiteral { .. }
        | InlineExpression::NumberLiteral { .. }
        | InlineExpression::MessageReference { .. } => {}
    }
}

fn collect_call_args(arguments: &fluent_syntax::ast::CallArguments<&str>, out: &mut Vec<String>) {
    for positional in &arguments.positional {
        collect_from_inline(positional, out);
    }
    for named in &arguments.named {
        collect_from_inline(&named.value, out);
    }
}

fn push_unique(out: &mut Vec<String>, name: &str) {
    if !out.iter().any(|v| v == name) {
        out.push(name.to_string());
    }
}

pub fn generate_l10n_accessors(
    messages: &[L10nMessage],
    resolver_path: &str,
    fluent_args_path: &str,
    fluent_value_path: &str,
) -> TokenStream {
    let resolver_path: SynPath = syn::parse_str(resolver_path)
        .unwrap_or_else(|e| panic!("invalid resolver_path {resolver_path:?}: {e}"));
    let fluent_args_path: SynPath = syn::parse_str(fluent_args_path)
        .unwrap_or_else(|e| panic!("invalid fluent_args_path {fluent_args_path:?}: {e}"));
    let fluent_value_path: SynPath = syn::parse_str(fluent_value_path)
        .unwrap_or_else(|e| panic!("invalid fluent_value_path {fluent_value_path:?}: {e}"));

    let methods = messages.iter().map(|msg| {
        let method_name = format_ident!("{}", msg.id.replace(['-', '.'], "_"));
        let id = &msg.id;
        
        let params = msg.variables.iter().map(|v| {
            let param = format_ident!("{}", v);
            quote! { #param: impl Into<#fluent_value_path<'static>> }
        });

        if msg.variables.is_empty() {
            quote! {
                pub fn #method_name(&self) -> String {
                    self.get_raw(#id, &#fluent_args_path::new())
                }
            }
        } else {
            let inserts = msg.variables.iter().map(|v| {
                let param = format_ident!("{}", v);
                quote! { args.set(#v, #param.into()); }
            });
            quote! {
                pub fn #method_name(&self, #(#params),*) -> String {
                    let mut args = #fluent_args_path::new();
                    #(#inserts)*
                    self.get_raw(#id, &args)
                }
            }
        }
    });

    quote! {
        #[allow(dead_code)]
        impl #resolver_path {
            #(#methods)*
        }
    }
}

fn discover_locale_files(locales_dir: &Path, reference_locale: &str) -> Vec<std::path::PathBuf> {
    let flat = locales_dir.join(format!("{reference_locale}.ftl"));
    if flat.is_file() {
        return vec![flat];
    }

    let nested = locales_dir.join(reference_locale);
    if nested.is_dir() {
        let mut files: Vec<_> = walkdir::WalkDir::new(&nested)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_file())
            .filter(|e| e.path().extension().is_some_and(|ext| ext == "ftl"))
            .map(|e| e.into_path())
            .collect();
        if !files.is_empty() {
            files.sort();
            return files;
        }
    }

    panic!(
        "no `.ftl` files found for locale {reference_locale:?} under {} - expected either {} or {}/**/*.ftl",
        locales_dir.display(),
        flat.display(),
        nested.display(),
    );
}

pub fn parse_locale_messages(locales_dir: &Path, reference_locale: &str) -> Vec<L10nMessage> {
    let mut messages: Vec<L10nMessage> = discover_locale_files(locales_dir, reference_locale)
        .iter()
        .flat_map(|path| parse_messages(path))
        .collect();
    messages.sort_by(|a, b| a.id.cmp(&b.id));
    messages
}

pub fn write_accessors(locales_dir: &Path, reference_locale: &str, out_path: &Path) {
    let messages = parse_locale_messages(locales_dir, reference_locale);
    let generated = generate_l10n_accessors(
        &messages,
        "crate::l10n::L10n",
        "crate::l10n::Args",
        "fluent_bundle::FluentValue",
    );
    fs::write(out_path, generated.to_string())
        .unwrap_or_else(|e| panic!("failed to write {}: {e}", out_path.display()));
}

pub fn build(locales_dir: impl AsRef<Path>) {
    build_for_locale(locales_dir, "en")
}

pub fn build_for_locale(locales_dir: impl AsRef<Path>, reference_locale: &str) {
    let locales_dir = locales_dir.as_ref();
    println!("cargo:rerun-if-changed={}", locales_dir.display());

    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    write_accessors(
        locales_dir,
        reference_locale,
        &Path::new(&out_dir).join("l10n_accessors.rs"),
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_ftl(content: &str) -> tempfile::NamedTempFile {
        let mut file = tempfile::NamedTempFile::new().expect("failed to create temp .ftl file");
        file.write_all(content.as_bytes()).expect("failed to write .ftl content");
        file
    }

    fn write_file(dir: &Path, rel: &str, content: &str) {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, content).unwrap();
    }

    #[test]
    fn plain_message_has_no_variables() {
        let file = write_ftl("hello-world = Hello, World!\n");
        let messages = parse_messages(file.path());
        assert_eq!(
            messages,
            vec![L10nMessage { id: "hello-world".into(), variables: vec![] }]
        );
    }

    #[test]
    fn placeable_variable_is_collected() {
        let file = write_ftl("welcome = Welcome, { $userName }.\n");
        let messages = parse_messages(file.path());
        assert_eq!(
            messages,
            vec![L10nMessage { id: "welcome".into(), variables: vec!["userName".into()] }]
        );
    }

    #[test]
    fn selector_and_variant_variables_are_all_collected_once_each() {
        let file = write_ftl(
            "emails = { $count ->\n    [one] { $count } email\n   *[other] { $count } emails, from { $sender }\n}\n",
        );
        let messages = parse_messages(file.path());
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].id, "emails");
        assert_eq!(messages[0].variables, vec!["count".to_string(), "sender".to_string()]);
    }

    #[test]
    fn non_message_entries_are_skipped() {
        let file = write_ftl("-brand-name = Nightly\n## a comment\nreal-message = Value\n");
        let messages = parse_messages(file.path());
        assert_eq!(
            messages,
            vec![L10nMessage { id: "real-message".into(), variables: vec![] }]
        );
    }

    #[test]
    fn generates_zero_arg_method_for_plain_message() {
        let messages = vec![L10nMessage { id: "hello-world".into(), variables: vec![] }];
        let generated = generate_l10n_accessors(
            &messages,
            "crate::l10n::L10n",
            "fluent_bundle::FluentArgs",
            "fluent_bundle::FluentValue",
        )
        .to_string();

        assert!(generated.contains("fn hello_world (& self) -> String"));
        assert!(generated.contains("fluent_bundle :: FluentArgs :: new"));
        assert!(generated.contains("get_raw (\"hello-world\""));
    }

    #[test]
    fn generates_one_param_per_variable_in_first_seen_order() {
        let messages = vec![L10nMessage {
            id: "emails".into(),
            variables: vec!["count".into(), "sender".into()],
        }];
        let generated = generate_l10n_accessors(
            &messages,
            "crate::l10n::L10n",
            "fluent_bundle::FluentArgs",
            "fluent_bundle::FluentValue",
        )
        .to_string();

        assert!(generated.contains("fn emails"), "{generated}");
        assert!(generated.contains("count : impl Into"), "{generated}");
        assert!(generated.contains("sender : impl Into"), "{generated}");
        assert!(generated.contains("args . set (\"count\""), "{generated}");
        assert!(generated.contains("args . set (\"sender\""), "{generated}");
        assert!(
            generated.find("count").unwrap() < generated.find("sender").unwrap(),
            "params must appear in first-seen order: {generated}"
        );
    }

    #[test]
    fn flat_layout_finds_locale_dot_ftl() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "en.ftl", "hello-world = Hello, World!\n");
        write_file(dir.path(), "ru.ftl", "hello-world = Привет, мир!\n");

        let messages = parse_locale_messages(dir.path(), "en");
        assert_eq!(
            messages,
            vec![L10nMessage { id: "hello-world".into(), variables: vec![] }]
        );
    }

    #[test]
    fn nested_layout_merges_every_ftl_file_under_the_locale_dir() {
        let dir = tempfile::tempdir().unwrap();
        write_file(dir.path(), "en/main.ftl", "hello-world = Hello, World!\n");
        write_file(dir.path(), "en/extra.ftl", "goodbye = Goodbye!\n");
        write_file(dir.path(), "ru/main.ftl", "hello-world = Привет, мир!\n");

        let messages = parse_locale_messages(dir.path(), "en");
        assert_eq!(
            messages,
            vec![
                L10nMessage { id: "goodbye".into(), variables: vec![] },
                L10nMessage { id: "hello-world".into(), variables: vec![] },
            ]
        );
    }

    #[test]
    #[should_panic(expected = "no `.ftl` files found for locale")]
    fn missing_locale_panics_with_both_expected_layouts() {
        let dir = tempfile::tempdir().unwrap();
        parse_locale_messages(dir.path(), "en");
    }
}

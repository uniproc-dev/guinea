use fluent_syntax::ast::{Entry, PatternElement};
use fluent_syntax::parser::parse;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::fs;
use std::path::Path;
use syn::{Ident, Path as SynPath};

pub fn parse_messages(ftl_path: &Path) -> Vec<(String, String)> {
    let content = fs::read_to_string(ftl_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", ftl_path.display()));
    let resource = parse(content.as_str())
        .unwrap_or_else(|(_, errors)| panic!("failed to parse {}: {errors:?}", ftl_path.display()));

    let mut messages: Vec<(String, String)> = resource
        .body
        .into_iter()
        .filter_map(|entry| match entry {
            Entry::Message(msg) => {
                let id = msg.id.name.to_string();
                let text = msg
                    .value
                    .map(|pattern| {
                        pattern
                            .elements
                            .into_iter()
                            .map(|el| match el {
                                PatternElement::TextElement { value } => value.to_string(),
                                PatternElement::Placeable { .. } => panic!(
                                    "message `{id}` uses a Fluent placeable ({{ ... }}) - \
                                     parse_messages only supports plain static text today"
                                ),
                            })
                            .collect::<String>()
                    })
                    .unwrap_or_default();
                Some((id, text))
            }
            _ => None,
        })
        .collect();
    messages.sort_by(|a, b| a.0.cmp(&b.0));
    messages
}

pub fn parse_message_ids(ftl_path: &Path) -> Vec<String> {
    parse_messages(ftl_path).into_iter().map(|(id, _)| id).collect()
}

fn field_ident_for(id: &str) -> Ident {
    format_ident!("{}", id.replace(['-', '.'], "_"))
}

pub fn generate_strings_contract(messages: &[(String, String)]) -> TokenStream {
    let fields = messages.iter().map(|(id, _)| {
        let field = field_ident_for(id);
        quote! { pub #field: String }
    });
    let ids = messages.iter().map(|(id, _)| id);
    let default_pairs = messages.iter().map(|(id, text)| quote! { (#id, #text) });

    quote! {
        #[derive(Clone, Debug)]
        pub struct L10nStrings {
            #(#fields),*
        }

        pub const L10N_MESSAGE_IDS: &[&str] = &[#(#ids),*];
        pub const L10N_MESSAGE_DEFAULTS: &[(&str, &str)] = &[#(#default_pairs),*];
    }
}

pub fn generate_strings_builder(ids: &[String], struct_path: &str, loader_expr: &str) -> TokenStream {
    let struct_path: SynPath = syn::parse_str(struct_path)
        .unwrap_or_else(|e| panic!("invalid struct_path {struct_path:?}: {e}"));
    let loader_path: SynPath = syn::parse_str(loader_expr)
        .unwrap_or_else(|e| panic!("invalid loader_expr {loader_expr:?}: {e}"));

    let field_inits = ids.iter().map(|id| {
        let field = field_ident_for(id);
        quote! { #field: #loader_path.lookup(&lang, #id) }
    });

    quote! {
        pub fn build_l10n_strings() -> #struct_path {
            let lang = unic_langid::langid!("en");
            #struct_path {
                #(#field_inits),*
            }
        }
    }
}

pub fn generate_slint_global_content(defaults: &[(String, String)]) -> String {
    let properties = defaults
        .iter()
        .map(|(id, text)| format!("    in property <string> {id}: \"{}\";", escape_slint_string(text)))
        .collect::<Vec<_>>()
        .join("\n");

    format!("// AUTO-GENERATED — do not edit manually\nexport global L10n {{\n{properties}\n}}\n")
}

fn escape_slint_string(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
}

pub fn generate_load_body_content(ids: &[String]) -> String {
    let pairs = ids
        .iter()
        .map(|id| {
            let field = id.replace(['-', '.'], "_");
            format!("({field}, set_{field})")
        })
        .collect::<Vec<_>>()
        .join(", ");

    format!("guinea_core::l10n_push_fields!(l10n, strings; {pairs})")
}

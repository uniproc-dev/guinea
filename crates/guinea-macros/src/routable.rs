//! `#[derive(Routable)]` - a URI path resolves to a typed enum variant, not
//! a string a `match` somewhere has to keep in sync by hand. Scope for this
//! pass: literal segments plus single `:name` captures (one per variant
//! field, matched by field name), no nested/wildcard segments yet - that's
//! layered on later without changing the derive's shape.

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, LitStr, Meta, parse_macro_input};

struct RouteVariant {
    ident: syn::Ident,
    segments: Vec<Segment>,
    field_names: Vec<syn::Ident>,
}

enum Segment {
    Literal(String),
    Capture(String),
}

pub fn derive_routable_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let enum_ident = &input.ident;

    let Data::Enum(data) = &input.data else {
        return syn::Error::new_spanned(&input, "#[derive(Routable)] only supports enums")
            .to_compile_error()
            .into();
    };

    let variants: Vec<RouteVariant> = data
        .variants
        .iter()
        .map(|variant| {
            let route_attr = variant
                .attrs
                .iter()
                .find(|a| a.path().is_ident("route"))
                .unwrap_or_else(|| panic!("variant {} is missing #[route(\"...\")]", variant.ident));

            let pattern: LitStr = match &route_attr.meta {
                Meta::List(list) => syn::parse2(list.tokens.clone())
                    .unwrap_or_else(|_| panic!("#[route(...)] on {} must be a string literal", variant.ident)),
                _ => panic!("#[route(...)] on {} must be a string literal", variant.ident),
            };

            let segments = pattern
                .value()
                .split('/')
                .filter(|s| !s.is_empty())
                .map(|s| {
                    if let Some(name) = s.strip_prefix(':') {
                        Segment::Capture(name.to_string())
                    } else {
                        Segment::Literal(s.to_string())
                    }
                })
                .collect();

            let field_names = match &variant.fields {
                Fields::Named(named) => named
                    .named
                    .iter()
                    .map(|f| f.ident.clone().unwrap())
                    .collect(),
                Fields::Unit => Vec::new(),
                Fields::Unnamed(_) => panic!(
                    "variant {} must use named fields to receive :captures",
                    variant.ident
                ),
            };

            RouteVariant {
                ident: variant.ident.clone(),
                segments,
                field_names,
            }
        })
        .collect();

    let path_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let field_pats: Vec<_> = v.field_names.iter().collect();
        let parts = v.segments.iter().map(|seg| match seg {
            Segment::Literal(lit) => quote! { #lit.to_string() },
            Segment::Capture(name) => {
                let field = format_ident!("{}", name);
                quote! { #field.to_string() }
            }
        });
        if field_pats.is_empty() {
            quote! {
                #enum_ident::#ident => {
                    let parts: Vec<String> = vec![#(#parts),*];
                    format!("/{}", parts.join("/"))
                }
            }
        } else {
            quote! {
                #enum_ident::#ident { #(#field_pats),* } => {
                    let parts: Vec<String> = vec![#(#parts),*];
                    format!("/{}", parts.join("/"))
                }
            }
        }
    });

    let parse_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let expected_len = v.segments.len();
        let mut binds = Vec::new();
        let checks = v.segments.iter().enumerate().map(|(i, seg)| match seg {
            Segment::Literal(lit) => quote! { parts[#i] == #lit },
            Segment::Capture(name) => {
                let field = format_ident!("{}", name);
                binds.push(quote! {
                    let #field = parts[#i].parse().ok()?;
                });
                quote! { true }
            }
        });
        let field_inits: Vec<_> = v.field_names.iter().collect();
        let construct = if field_inits.is_empty() {
            quote! { #enum_ident::#ident }
        } else {
            quote! { #enum_ident::#ident { #(#field_inits),* } }
        };

        quote! {
            if parts.len() == #expected_len && [#(#checks),*].iter().all(|c| *c) {
                #(#binds)*
                return Some(#construct);
            }
        }
    });

    let expanded = quote! {
        impl #enum_ident {
            pub fn path(&self) -> String {
                match self {
                    #(#path_arms),*
                }
            }

            pub fn parse(path: &str) -> Option<Self> {
                let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
                #(#parse_arms)*
                None
            }
        }
    };

    expanded.into()
}

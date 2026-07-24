use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, LitStr, Meta, parse_macro_input};

fn guinea_crate_path() -> proc_macro2::TokenStream {
    match crate_name("guinea") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(::guinea),
    }
}

struct RouteVariant {
    ident: syn::Ident,
    segments: Vec<Segment>,
    field_names: Vec<syn::Ident>,
    // Root -> immediate-parent segment types from `#[layout(...)]` scopes open
    // at this variant's position (see the stack walk in `derive_routable_impl`).
    ancestors: Vec<syn::Type>,
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

    // Ordered walk carrying a stack of currently-open `#[layout(...)]` scopes:
    // Dioxus's own nesting convention, adapted to plain enum variants (no
    // dedicated nested syntax needed). See the module doc for the two-phase
    // push/snapshot/pop per variant.
    let mut layout_stack: Vec<syn::Type> = Vec::new();
    let mut variants: Vec<RouteVariant> = Vec::new();
    for variant in &data.variants {
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

        let mut end_layout_count = 0usize;
        for attr in &variant.attrs {
            if attr.path().is_ident("layout") {
                let ty: syn::Type = attr
                    .parse_args()
                    .unwrap_or_else(|e| panic!("#[layout(...)] on {}: {}", variant.ident, e));
                layout_stack.push(ty);
            } else if attr.path().is_ident("end_layout") {
                end_layout_count += 1;
            }
        }
        
        let ancestors = layout_stack.clone();
        for _ in 0..end_layout_count {
            layout_stack.pop();
        }

        variants.push(RouteVariant {
            ident: variant.ident.clone(),
            segments,
            field_names,
            ancestors,
        });
    }

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

    let guinea = guinea_crate_path();

    
    let chain_consts = variants.iter().map(|v| {
        let const_name = format_ident!("__routable_chain_{}_{}", enum_ident, v.ident);
        
        let leaf_ident = &v.ident;
        
        let ancestor_entries = v
            .ancestors
            .iter()
            .map(|ty| quote! { #guinea::router::layout_entry::<#ty>() });
        let len = v.ancestors.len() + 1;
        quote! {
            #[allow(non_upper_case_globals)]
            const #const_name: [#guinea::router::SegmentEntry; #len] = [
                #(#ancestor_entries,)*
                #guinea::router::segment_entry::<#leaf_ident>(),
            ];
        }
    });

    let chain_arms = variants.iter().map(|v| {
        let ident = &v.ident;
        let const_name = format_ident!("__routable_chain_{}_{}", enum_ident, v.ident);
        if v.field_names.is_empty() {
            quote! { #enum_ident::#ident => &#const_name }
        } else {
            quote! { #enum_ident::#ident { .. } => &#const_name }
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

        #(#chain_consts)*

        impl #guinea::router::RouteChain for #enum_ident {
            fn chain(&self) -> &'static [#guinea::router::SegmentEntry] {
                match self {
                    #(#chain_arms),*
                }
            }
        }
    };

    expanded.into()
}

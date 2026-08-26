use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Ident, TokenStream};
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};

use guinea_route_dsl::{Segment, parse_pattern, type_ident};

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

/// Where the router's own types live.
///
/// An application usually reaches them through the facade, but a backend crate
/// depends on `guinea-router` directly and has no facade at all - and the
/// generated `RouteChain` impl has to name the same types either way.
fn router_path(guinea: &TokenStream) -> TokenStream {
    match crate_name("guinea-router") {
        Ok(FoundCrate::Itself) => quote!(crate::router),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident::router)
        }
        Err(_) => quote!(#guinea::router),
    }
}

/// Where `AppUri` lives, by the same reasoning.
fn core_path(guinea: &TokenStream) -> TokenStream {
    match crate_name("guinea-core") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(#guinea),
    }
}

/// The module a backend keeps `segment_entry`/`layout_entry` in: the parent of
/// the backend type, since it lives next to what it builds for.
fn backend_module(ty: &syn::Type) -> TokenStream {
    let syn::Type::Path(path) = ty else {
        panic!("routes!: backend must be a path, like `guinea_ratatui::Tui`");
    };

    let mut module = path.path.clone();
    module.segments.pop();
    let module: Vec<syn::PathSegment> = module.segments.into_iter().collect();
    if module.is_empty() {
        panic!("routes!: backend needs the module too, as in `guinea_ratatui::Tui`");
    }
    quote!(#(#module)::*)
}

pub fn routes_impl(input: TokenStream1) -> TokenStream1 {
    let tree = guinea_route_dsl::parse(input.into());
    let enum_ident = tree.name.clone();
    let leaves = tree.leaves();

    let variant_idents: Vec<Ident> = leaves.iter().map(|l| type_ident(&l.ty)).collect();

    let variant_defs = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let field_defs = leaf.fields.iter().map(|(name, ty)| quote! { #name: #ty });
        quote! { #ident { #(#field_defs),* } }
    });

    let path_arms = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let field_pats: Vec<&Ident> = leaf.fields.iter().map(|(name, _)| name).collect();
        let segments = parse_pattern(&leaf.pattern);
        let parts = segments.iter().map(|seg| match seg {
            Segment::Literal(lit) => quote! { #lit.to_string() },
            Segment::Capture(name) => {
                let field = format_ident!("{}", name);
                quote! { #field.to_string() }
            }
        });

        let pattern = quote! { #enum_ident::#ident { #(#field_pats),* } };
        quote! {
            #pattern => {
                let parts: Vec<String> = vec![#(#parts),*];
                format!("/{}", parts.join("/"))
            }
        }
    });

    let parse_arms = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let segments = parse_pattern(&leaf.pattern);
        let expected_len = segments.len();
        let mut binds = Vec::new();
        let checks = segments.iter().enumerate().map(|(i, seg)| match seg {
            Segment::Literal(lit) => quote! { parts[#i] == #lit },
            Segment::Capture(name) => {
                let field = format_ident!("{}", name);
                binds.push(quote! { let #field = parts[#i].parse().ok()?; });
                quote! { true }
            }
        });
        let field_inits: Vec<&Ident> = leaf.fields.iter().map(|(name, _)| name).collect();
        let construct = quote! { #enum_ident::#ident { #(#field_inits),* } };
        quote! {
            if parts.len() == #expected_len && [#(#checks),*].iter().all(|c| *c) {
                #(#binds)*
                return Some(#construct);
            }
        }
    });

    let guinea = guinea_crate_path();
    // Default to the facade's backend, so an application that has only one
    // never mentions it.
    let (backend_ty, backend_mod) = match &tree.backend {
        Some(ty) => (quote!(#ty), backend_module(ty)),
        None => (quote!(#guinea::Backend), quote!(#guinea::backend)),
    };
    let router = router_path(&guinea);
    let core = core_path(&guinea);

    let chain_consts = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let const_name = format_ident!("__routes_chain_{}_{}", enum_ident, ident);
        let leaf_ty = &leaf.ty;
        let ancestor_entries = leaf.ancestors.iter().map(|ty| {
            quote! { #backend_mod::layout_entry::<#ty>() }
        });
        let len = leaf.ancestors.len() + 1;
        quote! {
            #[allow(non_upper_case_globals)]
            const #const_name: [#router::SegmentEntry<#backend_ty>; #len] = [
                #(#ancestor_entries,)*
                #backend_mod::segment_entry::<#leaf_ty>(),
            ];
        }
    });

    let chain_arms = leaves.iter().zip(&variant_idents).map(|(_leaf, ident)| {
        let const_name = format_ident!("__routes_chain_{}_{}", enum_ident, ident);
        quote! { #enum_ident::#ident { .. } => &#const_name }
    });

    let expanded = quote! {
        #[derive(Clone, Debug, PartialEq)]
        pub enum #enum_ident {
            #(#variant_defs),*
        }

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

        impl #router::RouteChain<#backend_ty> for #enum_ident {
            fn chain(&self) -> &'static [#router::SegmentEntry<#backend_ty>] {
                match self {
                    #(#chain_arms),*
                }
            }
        }

        impl #router::ToUri for #enum_ident {
            fn to_uri(&self) -> #core::uri::AppUri {
                #core::uri::AppUri::parse(self.path())
                    .expect("routes!-derived path is always a valid PathAndQuery")
            }
        }
    };

    expanded.into()
}

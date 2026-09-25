//! `feature!` - a feature's manifest: its name and what it exports.
//!
//! ```ignore
//! feature! {
//!     pub AgentLink {
//!         exports { AgentLinkState }
//!     }
//! }
//! ```
//!
//! A list, not a struct: the manifest says what the feature is, and the
//! feature itself is what `#[installs]` builds. The type it makes holds one
//! `Bound` per export, in the order listed - `AgentLink(link)` - so the only
//! way to have one is to have claimed each thing it exports.

use proc_macro::TokenStream as TokenStream1;
use proc_macro2::TokenStream;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Attribute, Generics, Ident, Token, Type, Visibility, braced};

struct Manifest {
    attrs: Vec<Attribute>,
    vis: Visibility,
    name: Ident,
    generics: Generics,
    exports: Vec<Type>,
}

impl Parse for Manifest {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let attrs = input.call(Attribute::parse_outer)?;
        let vis: Visibility = input.parse()?;
        let name: Ident = input.parse()?;
        let mut generics: Generics = input.parse()?;
        generics.where_clause = input.parse()?;

        let body;
        braced!(body in input);

        let mut exports = Vec::new();
        while !body.is_empty() {
            let section: Ident = body.parse()?;
            if section != "exports" {
                return Err(syn::Error::new(
                    section.span(),
                    format!("unknown section `{section}`; a feature lists `exports`"),
                ));
            }

            let listed;
            braced!(listed in body);
            let types: Punctuated<Type, Token![,]> =
                listed.parse_terminated(Type::parse, Token![,])?;
            exports.extend(types);
        }

        Ok(Manifest {
            attrs,
            vis,
            name,
            generics,
            exports,
        })
    }
}

pub fn feature_impl(input: TokenStream1) -> TokenStream1 {
    let manifest = syn::parse_macro_input!(input as Manifest);
    expand(manifest).into()
}

fn expand(manifest: Manifest) -> TokenStream {
    let Manifest {
        attrs,
        vis,
        name,
        generics,
        exports,
    } = manifest;

    if exports.is_empty() && !generics.params.is_empty() {
        return syn::Error::new(
            name.span(),
            "a generic feature holds what it exports, and this one exports nothing to hold its \
             parameters - list what it exports, or drop the parameters",
        )
        .to_compile_error();
    }

    let gc = crate::handler::guinea_core_crate_path();
    let feature = crate::segment::context_path();
    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();

    let declared = if exports.is_empty() {
        quote! {
            #(#attrs)*
            #vis struct #name;
        }
    } else {
        let fields = exports
            .iter()
            .map(|export| quote!(#vis #gc::feature::Bound<#export>));
        quote! {
            #(#attrs)*
            #vis struct #name #generics (#(#fields),*) #where_clause;
        }
    };

    quote! {
        #declared

        impl #impl_generics #feature::Manifest for #name #type_generics #where_clause {
            type Exports = (#(#exports,)*);
        }
    }
}

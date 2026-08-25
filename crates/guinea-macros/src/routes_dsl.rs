use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use winnow::combinator::{alt, preceded, separated};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::{any, take_till};

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

enum Segment {
    Literal(String),
    Capture(String),
}

enum Node {
    Layout { ty: syn::Type, children: Vec<Node> },
    Page {
        ty: syn::Type,
        pattern: String,
        fields: Vec<(Ident, syn::Type)>,
    },
}
struct Leaf {
    ancestors: Vec<syn::Type>,
    ty: syn::Type,
    pattern: String,
    fields: Vec<(Ident, syn::Type)>,
}

type Tokens<'i> = &'i [TokenTree];

fn fail<O>() -> ModalResult<O> {
    Err(ErrMode::Backtrack(ContextError::new()))
}

fn kw<'i>(word: &'static str) -> impl Parser<Tokens<'i>, (), ErrMode<ContextError>> {
    move |input: &mut Tokens<'i>| match any.parse_next(input)? {
        TokenTree::Ident(id) if id == word => Ok(()),
        _ => fail(),
    }
}

fn any_ident<'i>(input: &mut Tokens<'i>) -> ModalResult<Ident> {
    match any.parse_next(input)? {
        TokenTree::Ident(id) => Ok(id),
        _ => fail(),
    }
}

fn punct<'i>(ch: char) -> impl Parser<Tokens<'i>, (), ErrMode<ContextError>> {
    move |input: &mut Tokens<'i>| match any.parse_next(input)? {
        TokenTree::Punct(p) if p.as_char() == ch => Ok(()),
        _ => fail(),
    }
}

fn group_inner<'i>(delim: Delimiter) -> impl Parser<Tokens<'i>, Vec<TokenTree>, ErrMode<ContextError>> {
    move |input: &mut Tokens<'i>| match any.parse_next(input)? {
        TokenTree::Group(g) if g.delimiter() == delim => Ok(g.stream().into_iter().collect()),
        _ => fail(),
    }
}

fn string_lit<'i>(input: &mut Tokens<'i>) -> ModalResult<String> {
    match any.parse_next(input)? {
        TokenTree::Literal(lit) => {
            let s: syn::LitStr = syn::parse2(quote!(#lit)).map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
            Ok(s.value())
        }
        _ => fail(),
    }
}

fn parse_type<'i>(stop: impl Fn(&TokenTree) -> bool) -> impl Parser<Tokens<'i>, syn::Type, ErrMode<ContextError>> {
    move |input: &mut Tokens<'i>| {
        let mut collected = Vec::new();
        while let Some(tt) = input.first() {
            if stop(tt) {
                break;
            }
            collected.push(any.parse_next(input)?);
        }
        if collected.is_empty() {
            return fail();
        }
        let ts: TokenStream = collected.into_iter().collect();
        syn::parse2::<syn::Type>(ts).map_err(|_| ErrMode::Backtrack(ContextError::new()))
    }
}

fn is_comma(tt: &TokenTree) -> bool {
    matches!(tt, TokenTree::Punct(p) if p.as_char() == ',')
}

fn parse_fields(tokens: Vec<TokenTree>) -> Vec<(Ident, syn::Type)> {
    let mut slice: Tokens = &tokens;
    let mut fields = Vec::new();
    while !slice.is_empty() {
        let name = any_ident.parse_next(&mut slice).expect("field name in page(...) { ... }");
        punct(':').parse_next(&mut slice).expect("`:` after field name");
        let ty = parse_type(is_comma)
            .parse_next(&mut slice)
            .expect("field type in page(...) { ... }");
        fields.push((name, ty));
        let _ = punct(',').parse_next(&mut slice);
    }
    fields
}

fn parse_layout_node<'i>(input: &mut Tokens<'i>) -> ModalResult<Node> {
    kw("layout").parse_next(input)?;
    let paren_tokens = group_inner(Delimiter::Parenthesis).parse_next(input)?;
    let mut paren_slice: Tokens = &paren_tokens;
    let ty = parse_type(|_| false).parse_next(&mut paren_slice)?;

    let brace_tokens = group_inner(Delimiter::Brace).parse_next(input)?;
    let mut brace_slice: Tokens = &brace_tokens;
    let children = parse_nodes(&mut brace_slice)?;
    Ok(Node::Layout { ty, children })
}

fn parse_page_node<'i>(input: &mut Tokens<'i>) -> ModalResult<Node> {
    kw("page").parse_next(input)?;
    let paren_tokens = group_inner(Delimiter::Parenthesis).parse_next(input)?;
    let mut paren_slice: Tokens = &paren_tokens;
    let ty = parse_type(is_comma).parse_next(&mut paren_slice)?;
    punct(',').parse_next(&mut paren_slice)?;
    let pattern = string_lit.parse_next(&mut paren_slice)?;

    let fields = match input.first() {
        Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace => {
            let inner: Vec<TokenTree> = g.stream().into_iter().collect();
            any.parse_next(input)?;
            parse_fields(inner)
        }
        _ => Vec::new(),
    };

    Ok(Node::Page { ty, pattern, fields })
}

fn parse_node<'i>(input: &mut Tokens<'i>) -> ModalResult<Node> {
    alt((parse_layout_node, parse_page_node)).parse_next(input)
}

fn parse_nodes<'i>(input: &mut Tokens<'i>) -> ModalResult<Vec<Node>> {
    let mut nodes = Vec::new();
    while !input.is_empty() {
        nodes.push(parse_node.parse_next(input)?);
    }
    Ok(nodes)
}

fn flatten(nodes: &[Node], ancestors: &mut Vec<syn::Type>, leaves: &mut Vec<Leaf>) {
    for node in nodes {
        match node {
            Node::Layout { ty, children } => {
                ancestors.push(ty.clone());
                flatten(children, ancestors, leaves);
                ancestors.pop();
            }
            Node::Page { ty, pattern, fields } => {
                leaves.push(Leaf {
                    ancestors: ancestors.clone(),
                    ty: ty.clone(),
                    pattern: pattern.clone(),
                    fields: fields.clone(),
                });
            }
        }
    }
}

fn type_ident(ty: &syn::Type) -> Ident {
    match ty {
        syn::Type::Path(p) => p.path.segments.last().expect("non-empty type path").ident.clone(),
        _ => panic!("page(...)'s type must be a plain path (e.g. `Processes` or `pages::Processes`)"),
    }
}

fn segment<'i>(input: &mut &'i str) -> ModalResult<Segment> {
    alt((
        preceded(':', take_till(0.., '/')).map(|s: &str| Segment::Capture(s.to_string())),
        take_till(1.., '/').map(|s: &str| Segment::Literal(s.to_string())),
    ))
    .parse_next(input)
}

fn parse_pattern(pattern: &str) -> Vec<Segment> {
    let trimmed = pattern.trim_matches('/');
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut input = trimmed;
    separated(1.., segment, '/')
        .parse_next(&mut input)
        .unwrap_or_else(|_| panic!("invalid route pattern {pattern:?}"))
}

/// The backend a route tree mounts on, and the module its entry constructors
/// live in.
///
/// Written as one type - `backend = guinea_ratatui::Tui` - because that is
/// what an application knows. The module is its parent: a backend keeps
/// `segment_entry`/`layout_entry` next to the type they build for.
fn parse_backend(slice: &mut Tokens) -> Option<(TokenStream, TokenStream)> {
    let tokens = *slice;
    match tokens.first() {
        Some(TokenTree::Ident(id)) if id == "backend" => {}
        _ => return None,
    }
    match tokens.get(1) {
        Some(TokenTree::Punct(p)) if p.as_char() == '=' => {}
        _ => panic!("routes! expects `backend = path::To::Backend,`"),
    }

    let mut end = 2;
    while end < tokens.len() {
        match &tokens[end] {
            TokenTree::Punct(p) if p.as_char() == ',' => break,
            _ => end += 1,
        }
    }
    if end == tokens.len() {
        panic!("routes! expects `,` after the backend");
    }

    let ty: syn::Type = syn::parse2(tokens[2..end].iter().cloned().collect())
        .unwrap_or_else(|e| panic!("routes!: backend is not a type: {e}"));
    let syn::Type::Path(path) = &ty else {
        panic!("routes!: backend must be a path, like `guinea_ratatui::Tui`");
    };

    let mut module = path.path.clone();
    module.segments.pop();
    let module: Vec<syn::PathSegment> = module.segments.into_iter().collect();
    if module.is_empty() {
        panic!("routes!: backend needs the module too, as in `guinea_ratatui::Tui`");
    }

    *slice = &tokens[end + 1..];
    Some((quote!(#ty), quote!(#(#module)::*)))
}

pub fn routes_impl(input: TokenStream1) -> TokenStream1 {
    let input2: TokenStream = input.into();
    let tokens: Vec<TokenTree> = input2.into_iter().collect();
    let mut slice: Tokens = &tokens;

    let backend = parse_backend(&mut slice);

    let enum_ident = any_ident
        .parse_next(&mut slice)
        .unwrap_or_else(|_| panic!("routes! expects `EnumName {{ ... }}`"));
    let body_tokens = group_inner(Delimiter::Brace)
        .parse_next(&mut slice)
        .unwrap_or_else(|_| panic!("routes! expects a `{{ ... }}` body after `{enum_ident}`"));
    let mut body_slice: Tokens = &body_tokens;
    let top_nodes = parse_nodes(&mut body_slice).expect("failed to parse routes! body");

    let mut leaves = Vec::new();
    flatten(&top_nodes, &mut Vec::new(), &mut leaves);

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
    let (backend_ty, backend_mod) = backend
        .unwrap_or_else(|| (quote!(#guinea::Backend), quote!(#guinea::backend)));
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

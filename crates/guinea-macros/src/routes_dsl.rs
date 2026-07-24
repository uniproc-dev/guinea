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

pub fn routes_impl(input: TokenStream1) -> TokenStream1 {
    let input2: TokenStream = input.into();
    let tokens: Vec<TokenTree> = input2.into_iter().collect();
    let mut slice: Tokens = &tokens;

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
        let pattern = if field_pats.is_empty() {
            quote! { #enum_ident::#ident }
        } else {
            quote! { #enum_ident::#ident { #(#field_pats),* } }
        };
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

    let chain_consts = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let const_name = format_ident!("__routes_chain_{}_{}", enum_ident, ident);
        let leaf_ty = &leaf.ty;
        let ancestor_entries = leaf.ancestors.iter().map(|ty| quote! { #guinea::router::layout_entry::<#ty>() });
        let len = leaf.ancestors.len() + 1;
        quote! {
            #[allow(non_upper_case_globals)]
            const #const_name: [#guinea::router::SegmentEntry; #len] = [
                #(#ancestor_entries,)*
                #guinea::router::segment_entry::<#leaf_ty>(),
            ];
        }
    });

    let chain_arms = leaves.iter().zip(&variant_idents).map(|(leaf, ident)| {
        let const_name = format_ident!("__routes_chain_{}_{}", enum_ident, ident);
        if leaf.fields.is_empty() {
            quote! { #enum_ident::#ident => &#const_name }
        } else {
            quote! { #enum_ident::#ident { .. } => &#const_name }
        }
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

        impl #guinea::router::RouteChain for #enum_ident {
            fn chain(&self) -> &'static [#guinea::router::SegmentEntry] {
                match self {
                    #(#chain_arms),*
                }
            }
        }

        impl #guinea::router::ToUri for #enum_ident {
            fn to_uri(&self) -> #guinea::uri::AppUri {
                #guinea::uri::AppUri::parse(self.path())
                    .expect("routes!-derived path is always a valid PathAndQuery")
            }
        }
    };

    expanded.into()
}

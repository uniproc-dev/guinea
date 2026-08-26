//! The route tree `routes!` declares, as data.
//!
//! Two things read the same declaration. `guinea-macros` expands it into an
//! enum, its chains and its URIs; a backend's build script generates from it
//! whatever that backend needs a route tree spelled out in - for Slint, the
//! `.slint` that holds every page and the branch each route shows.
//!
//! Both have to agree on what the declaration means, which is why the reading
//! of it lives here rather than in either.

use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};
use quote::quote;
use winnow::combinator::{alt, preceded, separated};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::{any, take_till};

/// A piece of a route pattern: `/:host/processes` is a capture and a literal.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Segment {
    Literal(String),
    Capture(String),
}

/// A node of the declared tree.
#[derive(Clone, Debug)]
pub enum Node {
    Layout {
        ty: syn::Type,
        children: Vec<Node>,
    },
    Page {
        ty: syn::Type,
        pattern: String,
        fields: Vec<(Ident, syn::Type)>,
    },
}

/// One page, with the layouts it sits inside - the chain the router installs.
#[derive(Clone, Debug)]
pub struct Leaf {
    pub ancestors: Vec<syn::Type>,
    pub ty: syn::Type,
    pub pattern: String,
    pub fields: Vec<(Ident, syn::Type)>,
}

/// A parsed `routes!` declaration.
#[derive(Clone, Debug)]
pub struct RouteTree {
    /// The enum the macro generates, and the type a route is.
    pub name: Ident,
    /// `backend = path::To::Backend`, when the declaration named one.
    pub backend: Option<syn::Type>,
    pub nodes: Vec<Node>,
}

impl RouteTree {
    /// Every page, in declaration order, with its ancestors.
    pub fn leaves(&self) -> Vec<Leaf> {
        let mut leaves = Vec::new();
        flatten(&self.nodes, &mut Vec::new(), &mut leaves);
        leaves
    }

    /// The layouts in the tree, in declaration order, without repeats.
    pub fn layouts(&self) -> Vec<syn::Type> {
        let mut found = Vec::new();
        collect_layouts(&self.nodes, &mut found);
        found
    }
}

/// Parses the body of a `routes!` invocation.
///
/// Panics with the same messages the macro has always produced: this is the
/// macro's own reading, moved rather than rewritten.
pub fn parse(input: TokenStream) -> RouteTree {
    let tokens: Vec<TokenTree> = input.into_iter().collect();
    let mut slice: Tokens = &tokens;

    let backend = parse_backend(&mut slice);

    let name = any_ident
        .parse_next(&mut slice)
        .unwrap_or_else(|_| panic!("routes! expects `EnumName {{ ... }}`"));
    let body_tokens = group_inner(Delimiter::Brace)
        .parse_next(&mut slice)
        .unwrap_or_else(|_| panic!("routes! expects a `{{ ... }}` body after `{name}`"));

    let mut body_slice: Tokens = &body_tokens;
    let nodes = parse_nodes(&mut body_slice).expect("failed to parse routes! body");

    RouteTree {
        name,
        backend,
        nodes,
    }
}

/// Finds the first `routes! { ... }` in a source file's tokens.
///
/// For build scripts, which have the file and not the expansion. Returns
/// `None` when the file declares no route tree.
pub fn find_in_source(source: &str) -> Option<RouteTree> {
    let file: TokenStream = source.parse().ok()?;
    find_invocation(file).map(parse)
}

fn find_invocation(stream: TokenStream) -> Option<TokenStream> {
    let tokens: Vec<TokenTree> = stream.into_iter().collect();

    for (i, token) in tokens.iter().enumerate() {
        let TokenTree::Ident(name) = token else {
            continue;
        };
        if name != "routes" {
            continue;
        }
        match tokens.get(i + 1) {
            Some(TokenTree::Punct(p)) if p.as_char() == '!' => {}
            _ => continue,
        }
        if let Some(TokenTree::Group(body)) = tokens.get(i + 2) {
            return Some(body.stream());
        }
    }

    // Not at this level: a route tree may sit inside a module in the same file.
    tokens.into_iter().find_map(|token| match token {
        TokenTree::Group(group) => find_invocation(group.stream()),
        _ => None,
    })
}

/// The last segment of a page or layout's type path - `Processes` for
/// `pages::processes::Processes`.
pub fn type_ident(ty: &syn::Type) -> Ident {
    match ty {
        syn::Type::Path(p) => p
            .path
            .segments
            .last()
            .expect("non-empty type path")
            .ident
            .clone(),
        _ => panic!("page(...)'s type must be a plain path (e.g. `Processes` or `pages::Processes`)"),
    }
}

/// Splits a route pattern into its segments.
pub fn parse_pattern(pattern: &str) -> Vec<Segment> {
    let trimmed = pattern.trim_matches('/');
    if trimmed.is_empty() {
        return Vec::new();
    }
    let mut input = trimmed;
    separated(1.., segment, '/')
        .parse_next(&mut input)
        .unwrap_or_else(|_| panic!("invalid route pattern {pattern:?}"))
}

fn flatten(nodes: &[Node], ancestors: &mut Vec<syn::Type>, leaves: &mut Vec<Leaf>) {
    for node in nodes {
        match node {
            Node::Layout { ty, children } => {
                ancestors.push(ty.clone());
                flatten(children, ancestors, leaves);
                ancestors.pop();
            }
            Node::Page {
                ty,
                pattern,
                fields,
            } => {
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

fn collect_layouts(nodes: &[Node], found: &mut Vec<syn::Type>) {
    for node in nodes {
        if let Node::Layout { ty, children } = node {
            let name = type_ident(ty);
            if !found.iter().any(|seen| type_ident(seen) == name) {
                found.push(ty.clone());
            }
            collect_layouts(children, found);
        }
    }
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

fn group_inner<'i>(
    delim: Delimiter,
) -> impl Parser<Tokens<'i>, Vec<TokenTree>, ErrMode<ContextError>> {
    move |input: &mut Tokens<'i>| match any.parse_next(input)? {
        TokenTree::Group(g) if g.delimiter() == delim => Ok(g.stream().into_iter().collect()),
        _ => fail(),
    }
}

fn string_lit<'i>(input: &mut Tokens<'i>) -> ModalResult<String> {
    match any.parse_next(input)? {
        TokenTree::Literal(lit) => {
            let s: syn::LitStr =
                syn::parse2(quote!(#lit)).map_err(|_| ErrMode::Backtrack(ContextError::new()))?;
            Ok(s.value())
        }
        _ => fail(),
    }
}

fn parse_type<'i>(
    stop: impl Fn(&TokenTree) -> bool,
) -> impl Parser<Tokens<'i>, syn::Type, ErrMode<ContextError>> {
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
        let name = any_ident
            .parse_next(&mut slice)
            .expect("field name in page(...) { ... }");
        punct(':')
            .parse_next(&mut slice)
            .expect("`:` after field name");
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

    Ok(Node::Page {
        ty,
        pattern,
        fields,
    })
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

fn segment<'i>(input: &mut &'i str) -> ModalResult<Segment> {
    alt((
        preceded(':', take_till(0.., '/')).map(|s: &str| Segment::Capture(s.to_string())),
        take_till(1.., '/').map(|s: &str| Segment::Literal(s.to_string())),
    ))
    .parse_next(input)
}

/// The backend a route tree mounts on.
///
/// Written as one type - `backend = guinea_ratatui::Tui` - because that is
/// what an application knows.
fn parse_backend(slice: &mut Tokens) -> Option<syn::Type> {
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

    *slice = &tokens[end + 1..];
    Some(ty)
}

#[cfg(test)]
mod tests {
    use super::*;

    const DECLARATION: &str = r#"
        routes! {
            backend = guinea::slint::Slint,
            Route {
                layout(TabsLayout) {
                    page(Processes, "/:context/processes") { context: String }
                    page(Services, "/:context/services") { context: String }
                }
            }
        }
    "#;

    #[test]
    fn a_declaration_flattens_to_one_leaf_per_page() {
        let tree = find_in_source(DECLARATION).expect("a route tree");
        let leaves = tree.leaves();

        assert_eq!(tree.name, "Route");
        assert_eq!(leaves.len(), 2);
        assert_eq!(type_ident(&leaves[0].ty), "Processes");
        assert_eq!(
            leaves[0].ancestors.iter().map(type_ident).collect::<Vec<_>>(),
            vec!["TabsLayout"],
            "a page carries the layouts it sits inside"
        );
    }

    #[test]
    fn a_layout_wrapping_several_pages_is_listed_once() {
        let tree = find_in_source(DECLARATION).expect("a route tree");
        assert_eq!(
            tree.layouts().iter().map(type_ident).collect::<Vec<_>>(),
            vec!["TabsLayout"]
        );
    }

    #[test]
    fn patterns_split_into_literals_and_captures() {
        assert_eq!(
            parse_pattern("/:context/processes"),
            vec![
                Segment::Capture("context".to_string()),
                Segment::Literal("processes".to_string()),
            ]
        );
    }

    #[test]
    fn a_route_tree_is_found_inside_a_module() {
        let source = format!("mod routes {{ use super::*; {DECLARATION} }}");
        assert!(find_in_source(&source).is_some());
    }

    #[test]
    fn a_file_without_one_is_not_an_error() {
        assert!(find_in_source("fn main() {}").is_none());
    }
}

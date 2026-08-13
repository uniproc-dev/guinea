use proc_macro::TokenStream as TokenStream1;
use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use winnow::error::{ContextError, ErrMode};
use winnow::prelude::*;
use winnow::token::any;

use crate::handler::guinea_core_crate_path;

type Tokens<'i> = &'i [TokenTree];

struct Manifest {
    ident: Ident,
    generics: syn::Generics,
    self_ty: syn::Type,
    handlers: Vec<syn::Type>,
    publishes: Vec<syn::Type>,
    subscribes: Vec<syn::Type>,
}

pub fn actor_impl(input: TokenStream1) -> TokenStream1 {
    let tokens: Vec<TokenTree> = TokenStream::from(input).into_iter().collect();
    match parse_manifest(&tokens).and_then(expand) {
        Ok(ts) => ts.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn fail<O>() -> ModalResult<O> {
    Err(ErrMode::Backtrack(ContextError::new()))
}

fn any_ident<'i>(input: &mut Tokens<'i>) -> ModalResult<Ident> {
    match any.parse_next(input)? {
        TokenTree::Ident(id) => Ok(id),
        _ => fail(),
    }
}

fn braced<'i>(input: &mut Tokens<'i>) -> ModalResult<Vec<TokenTree>> {
    match any.parse_next(input)? {
        TokenTree::Group(g) if g.delimiter() == Delimiter::Brace => {
            Ok(g.stream().into_iter().collect())
        }
        _ => fail(),
    }
}

fn parse_manifest(tokens: &[TokenTree]) -> syn::Result<Manifest> {
    let brace_at = tokens
        .iter()
        .position(|tt| matches!(tt, TokenTree::Group(g) if g.delimiter() == Delimiter::Brace))
        .ok_or_else(|| {
            syn::Error::new(
                span_of(tokens),
                "expected `ActorType<..> { .. }` - the body brace is missing",
            )
        })?;

    let (header, rest) = tokens.split_at(brace_at);
    let (ident, generics) = parse_header(header)?;
    let (_, ty_generics, _) = generics.split_for_impl();
    let self_ty: syn::Type = syn::parse2(quote!(#ident #ty_generics))?;

    let mut manifest = Manifest {
        ident,
        generics,
        self_ty,
        handlers: Vec::new(),
        publishes: Vec::new(),
        subscribes: Vec::new(),
    };

    let TokenTree::Group(body) = &rest[0] else {
        unreachable!("position matched a brace group")
    };
    parse_sections(&body.stream().into_iter().collect::<Vec<_>>(), &mut manifest)?;

    if manifest.handlers.is_empty() {
        return Err(syn::Error::new(
            manifest.ident.span(),
            "an actor must declare at least one handler: `handlers { .. }`",
        ));
    }

    Ok(manifest)
}

/// The header reads like a struct declaration, so it is parsed as one: that
/// gives bounds and `where` clauses for free.
fn parse_header(header: &[TokenTree]) -> syn::Result<(Ident, syn::Generics)> {
    if header.is_empty() {
        return Err(syn::Error::new(
            Span::call_site(),
            "expected an actor type before the body",
        ));
    }
    let ts: TokenStream = header.iter().cloned().collect();
    let item: syn::ItemStruct = syn::parse2(quote!(struct #ts {}))
        .map_err(|e| syn::Error::new(span_of(header), format!("invalid actor type: {e}")))?;
    Ok((item.ident, item.generics))
}

fn parse_sections(tokens: &[TokenTree], manifest: &mut Manifest) -> syn::Result<()> {
    let mut slice: Tokens = tokens;

    while !slice.is_empty() {
        let name = any_ident
            .parse_next(&mut slice)
            .map_err(|_| syn::Error::new(span_of(slice), "expected a section name"))?;

        let body = braced
            .parse_next(&mut slice)
            .map_err(|_| syn::Error::new(name.span(), "expected `{ .. }` after the section name"))?;

        let types = parse_type_list(&body)?;

        match name.to_string().as_str() {
            "handlers" => manifest.handlers = types,
            "publishes" => manifest.publishes = types,
            "subscribes" => manifest.subscribes = types,
            other => {
                return Err(syn::Error::new(
                    name.span(),
                    format!("unknown section `{other}`; expected handlers, publishes or subscribes"),
                ));
            }
        }
    }

    Ok(())
}

fn parse_type_list(tokens: &[TokenTree]) -> syn::Result<Vec<syn::Type>> {
    let mut types = Vec::new();
    let mut current: Vec<TokenTree> = Vec::new();

    for tt in tokens {
        if matches!(tt, TokenTree::Punct(p) if p.as_char() == ',') {
            push_type(&mut current, &mut types)?;
        } else {
            current.push(tt.clone());
        }
    }
    push_type(&mut current, &mut types)?;

    Ok(types)
}

fn push_type(current: &mut Vec<TokenTree>, out: &mut Vec<syn::Type>) -> syn::Result<()> {
    if current.is_empty() {
        return Ok(());
    }
    let span = span_of(current);
    let ts: TokenStream = current.drain(..).collect();
    out.push(syn::parse2(ts).map_err(|e| syn::Error::new(span, format!("expected a type: {e}")))?);
    Ok(())
}

fn span_of(tokens: &[TokenTree]) -> Span {
    tokens
        .first()
        .map(|tt| tt.span())
        .unwrap_or_else(Span::call_site)
}

fn expand(manifest: Manifest) -> syn::Result<TokenStream> {
    let gc = guinea_core_crate_path();
    let Manifest {
        ident,
        generics,
        self_ty,
        handlers,
        publishes,
        subscribes,
    } = manifest;

    let (impl_generics, _, where_clause) = generics.split_for_impl();

    let handlers_marker = format_ident!("__Handlers_{}", ident);
    let signals_marker = format_ident!("__Signals_{}", ident);
    let bus_marker = format_ident!("__Bus_{}", ident);

    let handler_asserts = handlers.iter().map(|msg| {
        quote! { assert_handler::<#self_ty, #msg>(); }
    });

    let signal_impls = publishes.iter().map(|msg| {
        quote! { impl #gc::actor::traits::AllowedSignal<#msg> for #signals_marker {} }
    });

    let subscriptions = subscribes.iter().map(|msg| {
        quote! {
            <#msg as #gc::actor::event_bus::builder::EventSubscription<#self_ty>>::subscribe_into(
                addr.clone(),
                tracker,
            );
        }
    });

    let bus_ty = if subscribes.is_empty() {
        quote!(())
    } else {
        quote!(#bus_marker)
    };

    let bus_impl = if subscribes.is_empty() {
        quote!()
    } else {
        quote! {
            #[doc(hidden)]
            pub struct #bus_marker;

            impl #impl_generics #gc::actor::event_bus::builder::EventSubscription<#self_ty>
                for #bus_marker #where_clause
            {
                fn subscribe_into(
                    addr: #gc::actor::Addr<#self_ty>,
                    tracker: &impl #gc::lifecycle_tracker::LifecycleTracker,
                ) {
                    #(#subscriptions)*
                }
            }
        }
    };

    Ok(quote! {
        #[doc(hidden)]
        pub struct #handlers_marker;

        #[doc(hidden)]
        pub struct #signals_marker;

        #bus_impl

        impl #impl_generics #gc::actor::traits::DirectHandler<#self_ty>
            for #handlers_marker #where_clause {}

        #(#signal_impls)*

        impl #impl_generics #gc::actor::traits::ManagedActor for #self_ty #where_clause {
            type Bus = #bus_ty;
            type Handlers = #handlers_marker;
            type Signals = #signals_marker;
        }

        const _: () = {
            fn check_handlers #impl_generics () #where_clause {
                fn assert_handler<A, M>()
                where
                    A: #gc::actor::traits::Handler<M>,
                    M: #gc::actor::traits::Message,
                {
                }
                #(#handler_asserts)*
            }
        };
    })
}

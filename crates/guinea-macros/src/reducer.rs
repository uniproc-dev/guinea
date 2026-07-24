//! `#[reducer]` (fn -> feature marker + `impl Reducer`), the `#[dispatch]`
//! helper it reads, and `#[derive(ReducerState)]` (terse `Default`/`Clone`/
//! `PartialEq`/`Debug` for a reducer's state).

use proc_macro::TokenStream;
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Error, Fields, FnArg, ItemFn, Path, Type, parse_macro_input,
    spanned::Spanned,
};

pub fn reducer_impl(input: TokenStream) -> TokenStream {
    let func = parse_macro_input!(input as ItemFn);
    expand_reducer(func).unwrap_or_else(|e| e.to_compile_error().into())
}

fn expand_reducer(mut func: ItemFn) -> syn::Result<TokenStream> {
    // The fn name *is* the feature marker type.
    let marker = func.sig.ident.clone();

    if func.sig.inputs.len() != 2 {
        return Err(Error::new(
            func.sig.span(),
            "#[reducer] expects exactly two parameters: (state: &mut State, msg: Push)",
        ));
    }

    // param 0: `state: &mut State` -> State
    let state_arg = &func.sig.inputs[0];
    let state_ty = match arg_type(state_arg)? {
        Type::Reference(r) if r.mutability.is_some() => (*r.elem).clone(),
        other => {
            return Err(Error::new(
                other.span(),
                "#[reducer]'s first parameter must be `&mut State`",
            ));
        }
    };

    // param 1: `msg: Push` -> Push
    let push_ty = arg_type(&func.sig.inputs[1])?.clone();

    // `type Actions` from a sibling `#[dispatch(Trait)]`, else NoopActions.
    let dispatch = extract_dispatch(&func.attrs)?;
    func.attrs.retain(|a| !a.path().is_ident("dispatch"));
    let actions_ty = match dispatch {
        Some(trait_path) => {
            let adapter = dispatch_adapter_path(trait_path);
            quote!(#adapter)
        }
        None => quote!(guinea_core::scope::NoopActions),
    };

    // Reuse the original parameters (their names, e.g. `state`/`msg`) and the
    // body verbatim - the concrete types equal the associated types, so the
    // signature still satisfies `Reducer::reduce`.
    let param_state = &func.sig.inputs[0];
    let param_push = &func.sig.inputs[1];
    let body = &func.block;
    let vis = &func.vis;

    Ok(quote! {
        #[derive(Default)]
        #vis struct #marker;

        impl guinea_core::scope::Reducer for #marker {
            type State = #state_ty;
            type Push = #push_ty;
            type Actions = #actions_ty;

            fn reduce(#param_state, #param_push) #body
        }
    }
    .into())
}

fn arg_type(arg: &FnArg) -> syn::Result<&Type> {
    match arg {
        FnArg::Typed(pt) => Ok(&pt.ty),
        FnArg::Receiver(r) => Err(Error::new(
            r.span(),
            "#[reducer] is a free function, not a method - no `self` parameter",
        )),
    }
}

fn extract_dispatch(attrs: &[syn::Attribute]) -> syn::Result<Option<Path>> {
    for attr in attrs {
        if attr.path().is_ident("dispatch") {
            return attr.parse_args::<Path>().map(Some);
        }
    }
    Ok(None)
}

/// Maps an actions *trait* path (`…::ProcessesActions`) to the generated
/// store-dispatch-adapter type path (`…::ProcessesDispatch`) that `#[actions]`
/// emits alongside it - same module, last segment's `Actions` suffix renamed.
fn dispatch_adapter_path(mut path: Path) -> Path {
    if let Some(last) = path.segments.last_mut() {
        let name = last.ident.to_string();
        let base = name.strip_suffix("Actions").unwrap_or(&name);
        last.ident = format_ident!("{}Dispatch", base);
    }
    path
}

pub fn reducer_state_derive_impl(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_reducer_state(input).unwrap_or_else(|e| e.to_compile_error().into())
}

fn expand_reducer_state(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;

    if !input.generics.params.is_empty() {
        return Err(Error::new(
            input.generics.span(),
            "#[derive(ReducerState)] doesn't support generics - use \
             `#[derive(Default, Clone, PartialEq, Debug)]` directly",
        ));
    }

    let fields = match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(named) => &named.named,
            _ => {
                return Err(Error::new(
                    input.span(),
                    "#[derive(ReducerState)] supports named-field structs only",
                ));
            }
        },
        _ => {
            return Err(Error::new(
                input.span(),
                "#[derive(ReducerState)] supports structs only",
            ));
        }
    };

    let idents: Vec<_> = fields.iter().map(|f| f.ident.clone().unwrap()).collect();

    // PartialEq over zero fields is just `true`.
    let eq_body = if idents.is_empty() {
        quote!(true)
    } else {
        quote!( #( self.#idents == other.#idents )&&* )
    };

    Ok(quote! {
        impl Default for #name {
            fn default() -> Self {
                Self { #( #idents: ::core::default::Default::default() ),* }
            }
        }
        impl Clone for #name {
            fn clone(&self) -> Self {
                Self { #( #idents: ::core::clone::Clone::clone(&self.#idents) ),* }
            }
        }
        impl PartialEq for #name {
            fn eq(&self, other: &Self) -> bool {
                #eq_body
            }
        }
        impl ::core::fmt::Debug for #name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.debug_struct(stringify!(#name))
                    #( .field(stringify!(#idents), &self.#idents) )*
                    .finish()
            }
        }
    }
    .into())
}

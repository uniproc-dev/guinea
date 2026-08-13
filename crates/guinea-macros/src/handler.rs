use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::quote;
use syn::{
    Error, FnArg, GenericArgument, ItemFn, PatType, PathArguments, ReturnType, Result, Type,
    spanned::Spanned,
};

pub fn generate_standalone_handler(item: ItemFn) -> TokenStream {
    expand_handler(item).unwrap_or_else(|err| err.to_compile_error().into())
}

pub(crate) fn guinea_core_crate_path() -> proc_macro2::TokenStream {
    match crate_name("guinea-core") {
        Ok(FoundCrate::Itself) => quote!(crate),
        Ok(FoundCrate::Name(name)) => {
            let ident = syn::Ident::new(&name, proc_macro2::Span::call_site());
            quote!(::#ident)
        }
        Err(_) => quote!(::guinea_core),
    }
}

fn expand_handler(item: ItemFn) -> Result<TokenStream> {
    let gc = guinea_core_crate_path();
    let fn_name = &item.sig.ident;
    let is_async = item.sig.asyncness.is_some();
    let inputs = &item.sig.inputs;

    let (actor_ty, actor_name) = if is_async {
        extract_actor_from_async_ctx(inputs.get(0))?
    } else {
        extract_actor_from_ref(inputs.get(0))?
    };

    let msg_ty = if is_async {
        let (ty, _) = extract_msg_info(inputs.get(1))?;
        ty.clone()
    } else {
        extract_sync_msg(inputs.get(1))?
    };
    let msg_ty = &msg_ty;

    // A return type is the signal that this handler answers an
    // `AsyncBus::request` rather than reacting to a fire-and-forget event.
    // The generated impl is the only code path that can call
    // `AsyncBus::reply` for this message, so "replies exactly once" holds
    // by construction regardless of which of the four branches below fires.
    let ret_ty = match &item.sig.output {
        ReturnType::Default => None,
        ReturnType::Type(_, ty) => Some(ty),
    };

    let trait_generics = item.sig.generics.clone();
    let (impl_generics, _, where_clause) = trait_generics.split_for_impl();

    let trait_impl = match (is_async, ret_ty) {
        // Fire-and-forget sync handler - unchanged from before the RPC
        // heuristic existed.
        (false, None) => {
            if inputs.len() != 2 {
                return Err(Error::new(
                    item.sig.span(),
                    format!(
                        "Sync handler for actor '{}' must have exactly 2 arguments: (actor: &mut {}, ctx: Context<{}, Msg>)",
                        actor_name, actor_name, actor_name
                    ),
                ));
            }
            quote! {
                impl #impl_generics #gc::actor::Handler<#msg_ty> for #actor_ty #where_clause {
                    fn handle(&mut self, ctx: #gc::actor::Context<Self, #msg_ty>) {
                        #fn_name(self, ctx);
                    }
                }
            }
        }

        // Sync RPC handler - `RpcHandler<#msg_ty>`'s blanket `Handler<RpcRequest<#msg_ty>>`
        // impl (in guinea-core) calls `AsyncBus::reply` right after this
        // returns, so the value just needs to flow back out as an
        // expression.
        (false, Some(ret_ty)) => {
            if inputs.len() != 2 {
                return Err(Error::new(
                    item.sig.span(),
                    format!(
                        "Sync RPC handler for actor '{}' must have exactly 2 arguments: (actor: &mut {}, ctx: Context<{}, Req>)",
                        actor_name, actor_name, actor_name
                    ),
                ));
            }
            quote! {
                impl #impl_generics #gc::actor::event_bus::rpc::RpcHandler<#msg_ty> for #actor_ty #where_clause {
                    fn handle_rpc(&mut self, ctx: #gc::actor::Context<Self, #msg_ty>) -> #ret_ty {
                        #fn_name(self, ctx)
                    }
                }
            }
        }

        // Fire-and-forget async handler - unchanged from before the RPC
        // heuristic existed.
        (true, None) => {
            if inputs.len() != 2 {
                return Err(Error::new(
                    item.sig.span(),
                    format!(
                        "Async handler for actor '{}' must have exactly 2 arguments: (ctx: AsyncContext<{}>, msg: _)",
                        actor_name, actor_name
                    ),
                ));
            }
            quote! {
                impl #impl_generics #gc::actor::Handler<#msg_ty> for #actor_ty #where_clause {
                    fn handle(&mut self, ctx: #gc::actor::Context<Self, #msg_ty>) {
                        let actx = ctx.async_ctx();
                        let msg = ctx.msg;
                        tokio::spawn(async move {
                            #fn_name(actx, msg).await;
                        });
                    }
                }
            }
        }

        // Async RPC handler - the reply only becomes available after
        // `.await`, so `RpcHandler` (sync-only) doesn't fit. Implement
        // `Handler<RpcRequest<#msg_ty>>` directly instead: unwrap the
        // envelope up front, spawn the body, and reply with whatever it
        // returns once it resolves. The user's fn body never sees
        // `correlation_id` and has no way to call `reply` itself - this
        // generated glue is the only thing that can, same "exactly once"
        // guarantee as the sync branch, just paid for with generated code
        // instead of the trait system (async traits can't express "you
        // must eventually produce exactly one value" the way a plain
        // return type does).
        (true, Some(ret_ty)) => {
            if inputs.len() != 2 {
                return Err(Error::new(
                    item.sig.span(),
                    format!(
                        "Async RPC handler for actor '{}' must have exactly 2 arguments: (ctx: AsyncContext<{}>, req: _)",
                        actor_name, actor_name
                    ),
                ));
            }
            quote! {
                impl #impl_generics #gc::actor::Handler<#gc::actor::event_bus::RpcRequest<#msg_ty>> for #actor_ty #where_clause {
                    fn handle(&mut self, ctx: #gc::actor::Context<Self, #gc::actor::event_bus::RpcRequest<#msg_ty>>) {
                        let actx = ctx.async_ctx();
                        let correlation_id = ctx.msg.correlation_id;
                        let chain = ctx.msg.chain;
                        let msg = ctx.msg.payload;
                        #gc::actor::event_bus::AsyncBus::spawn_reply::<#ret_ty, _>(
                            correlation_id,
                            chain,
                            async move { #fn_name(actx, msg).await },
                        );
                    }
                }
            }
        }
    };

    Ok(TokenStream::from(quote! {
        #item

        #trait_impl
    }))
}

fn type_to_string(ty: &Type) -> String {
    quote!(#ty).to_string().replace(' ', "")
}

/// The message type is read off the second argument, which must be
/// `Context<Actor, Msg>`.
fn extract_sync_msg(arg: Option<&FnArg>) -> Result<Type> {
    let (ty, _) = extract_msg_info(arg)?;

    let malformed = || {
        Error::new(
            ty.span(),
            "a handler's second argument must be `Context<Actor, Msg>`",
        )
    };

    let Type::Path(path) = ty else {
        return Err(malformed());
    };
    let Some(last) = path.path.segments.last() else {
        return Err(malformed());
    };
    if last.ident != "Context" {
        return Err(malformed());
    }
    let syn::PathArguments::AngleBracketed(args) = &last.arguments else {
        return Err(malformed());
    };

    let types: Vec<Type> = args
        .args
        .iter()
        .filter_map(|arg| match arg {
            syn::GenericArgument::Type(ty) => Some(ty.clone()),
            _ => None,
        })
        .collect();

    match types.len() {
        0 | 1 => Err(malformed()),
        _ => Ok(types[1].clone()),
    }
}

fn extract_msg_info(arg: Option<&FnArg>) -> Result<(&Type, String)> {
    let arg =
        arg.ok_or_else(|| Error::new(proc_macro2::Span::call_site(), "Missing message argument"))?;
    if let FnArg::Typed(PatType { ty, .. }) = arg {
        Ok((ty.as_ref(), type_to_string(ty)))
    } else {
        Err(Error::new(
            arg.span(),
            "Expected a typed argument for message",
        ))
    }
}

fn extract_actor_from_ref(arg: Option<&FnArg>) -> Result<(&Type, String)> {
    let arg =
        arg.ok_or_else(|| Error::new(proc_macro2::Span::call_site(), "Missing actor argument"))?;
    if let FnArg::Typed(PatType { ty, .. }) = arg {
        if let Type::Reference(tr) = ty.as_ref() {
            let inner = tr.elem.as_ref();
            return Ok((inner, type_to_string(inner)));
        }
    }
    Err(Error::new(
        arg.span(),
        "First argument of a sync handler must be '&ActorType' or '&mut ActorType'",
    ))
}

fn extract_actor_from_async_ctx(arg: Option<&FnArg>) -> Result<(&Type, String)> {
    let arg = arg.ok_or_else(|| {
        Error::new(
            proc_macro2::Span::call_site(),
            "Missing AsyncContext argument",
        )
    })?;
    if let FnArg::Typed(PatType { ty, .. }) = arg {
        if let Type::Path(tp) = ty.as_ref() {
            let last_segment = tp
                .path
                .segments
                .last()
                .ok_or_else(|| Error::new(arg.span(), "Invalid path for AsyncContext"))?;

            if last_segment.ident == "AsyncContext" {
                if let PathArguments::AngleBracketed(args) = &last_segment.arguments {
                    if let Some(GenericArgument::Type(inner_ty)) = args.args.first() {
                        return Ok((inner_ty, type_to_string(inner_ty)));
                    }
                }
                return Err(Error::new(
                    arg.span(),
                    "AsyncContext must specify the Actor type in angle brackets: AsyncContext<MyActor>",
                ));
            }
        }
    }
    Err(Error::new(
        arg.span(),
        "First argument of an async handler must be 'AsyncContext<ActorType>'",
    ))
}

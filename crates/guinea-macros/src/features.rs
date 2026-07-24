use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use quote::quote;
use syn::{FnArg, ItemFn, Pat, parse_macro_input};

pub fn app_feature_impl(_args: TokenStream, input: TokenStream) -> TokenStream {
    let input_fn = parse_macro_input!(input as ItemFn);

    let vis = &input_fn.vis;
    let sig = &input_fn.sig;
    let func_name = &sig.ident;

    let struct_name_str = func_name.to_string().to_upper_camel_case();
    let struct_name = syn::Ident::new(&struct_name_str, func_name.span());

    let mut params_info = None;
    let mut args_iter = sig.inputs.iter();
    let _ctx_arg = args_iter.next();

    if let Some(FnArg::Typed(pat_type)) = args_iter.next() {
        if let Pat::Ident(pat_ident) = &*pat_type.pat {
            if pat_ident.ident == "params" {
                params_info = Some((&pat_ident.ident, &pat_type.ty));
            } else {
                panic!("app_feature must only have 'ctx' and optionally 'params' as arguments");
            }
        }
    }
    if args_iter.next().is_some() {
        panic!("app_feature can have at most two arguments: 'ctx' and 'params'");
    }

    let expanded = if let Some((_, params_ty)) = params_info {
        quote! {
            #input_fn

            #vis struct #struct_name {
                pub params: #params_ty,
            }

            impl #struct_name {
                pub fn new(params: #params_ty) -> Self {
                    Self { params }
                }
            }

            impl AppFeature for #struct_name {
                fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()> {
                    #func_name(ctx, self.params.clone())
                }
            }
        }
    } else {
        quote! {
            #input_fn

            #vis struct #struct_name;

            impl AppFeature for #struct_name {
                fn install(&mut self, ctx: &mut AppFeatureInitContext) -> anyhow::Result<()> {
                    #func_name(ctx)
                }
            }
        }
    };

    expanded.into()
}

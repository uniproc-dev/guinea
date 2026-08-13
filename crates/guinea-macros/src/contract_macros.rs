use proc_macro::TokenStream;
use syn::ItemTrait;

const HELPER_ATTRS: &[&str] = &["manual", "tracing"];

fn strip_helper_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| {
        !HELPER_ATTRS
            .iter()
            .any(|helper| attr.path().is_ident(helper))
    });
}

pub fn port_impl(mut trait_item: ItemTrait) -> TokenStream {
    let trait_ident = &trait_item.ident;

    for item in &mut trait_item.items {
        if let syn::TraitItem::Fn(method) = item {
            strip_helper_attrs(&mut method.attrs);
        }
    }

    let msg_ty = trait_item.items.iter().find_map(|item| {
        let syn::TraitItem::Fn(method) = item else {
            return None;
        };
        if method.sig.ident != "send" {
            return None;
        }
        method.sig.inputs.iter().find_map(|arg| match arg {
            syn::FnArg::Typed(pat_ty) => Some((*pat_ty.ty).clone()),
            _ => None,
        })
    });

    let blanket_impl = msg_ty.map(|msg_ty| {
        quote::quote! {
            impl<__F: Fn(#msg_ty) + 'static> #trait_ident for __F {
                fn send(&self, msg: #msg_ty) {
                    self(msg)
                }
            }
        }
    });

    quote::quote! {
        #trait_item
        #blanket_impl
    }
    .into()
}

use proc_macro::TokenStream;
use quote::quote;
use syn::{ItemEnum, ItemStruct};

const HELPER_ATTRS: &[&str] = &["manual", "tracing", "slint"];

fn strip_helper_attrs(attrs: &mut Vec<syn::Attribute>) {
    attrs.retain(|attr| {
        !HELPER_ATTRS
            .iter()
            .any(|helper| attr.path().is_ident(helper))
    });
}

pub fn slint_dto_impl(item: TokenStream) -> TokenStream {
    if let Ok(mut item_struct) = syn::parse::<ItemStruct>(item.clone()) {
        for field in &mut item_struct.fields {
            strip_helper_attrs(&mut field.attrs);
        }
        return quote!(#item_struct).into();
    }

    if let Ok(mut item_enum) = syn::parse::<ItemEnum>(item) {
        for variant in &mut item_enum.variants {
            strip_helper_attrs(&mut variant.attrs);
        }
        return quote!(#item_enum).into();
    }

    panic!("#[slint_dto] can only be applied to structs and enums");
}

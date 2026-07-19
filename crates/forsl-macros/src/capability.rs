use proc_macro::TokenStream;
use syn::{ItemStruct, LitStr};

pub fn capability_impl(attr: TokenStream, item: TokenStream) -> TokenStream {
    let key = syn::parse::<LitStr>(attr)
        .unwrap_or_else(|_| panic!("#[capability(\"key\")] requires a string literal key"))
        .value();
    let item_struct = syn::parse::<ItemStruct>(item)
        .unwrap_or_else(|_| panic!("#[capability(...)] can only be applied to a struct"));
    let struct_name = item_struct.ident.to_string();

    let registration = forsl_codegen::contracts::emit_capability_registration(&key, &struct_name);

    quote::quote! {
        #item_struct
        #registration
    }
    .into()
}

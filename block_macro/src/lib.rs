use proc_macro::TokenStream;
use quote::ToTokens;

#[proc_macro_attribute]
pub fn block(args: TokenStream, item: TokenStream) -> TokenStream {
    let size = match syn::parse_macro_input!(args as syn::AttributeArgs).as_slice() {
        &[syn::NestedMeta::Lit(syn::Lit::Int(ref int))] => {
            int.base10_parse::<usize>().expect("invalid block size")
        }
        &[_] => panic!("expected integer literal"),
        _ => panic!("unexpected number of arguments"),
    };

    let item = syn::parse_macro_input!(item as syn::DeriveInput);
    let ident = &item.ident;
    let derived = quote::quote! {
        impl ::block::Block for #ident {
            fn block_size() -> usize {
                #size
            }
        }
    };
    let mut item = item.to_token_stream();
    item.extend(derived);
    item.into()
}

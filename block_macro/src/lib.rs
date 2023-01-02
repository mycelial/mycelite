use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::ToTokens;

/// extract block size from attribute
///
/// for enums block size should not be specified, tag value is always u32 (due to serde)
fn extract_block_size(args: &syn::AttributeArgs) -> Option<usize> {
    match args.as_slice() {
        [] => None,
        [syn::NestedMeta::Lit(syn::Lit::Int(ref int))] => {
            Some(int.base10_parse::<usize>().expect("invalid block size"))
        }
        [_] => panic!("expected integer literal"),
        _ => panic!("unexpected number of arguments"),
    }
}

/// extact instance block size
///
/// for structs it's the same as a block size
/// for enums - for now only new-type enums are supported and each arm has size of inner element,
/// which should implement Block trait.
fn extract_instance_block_size(
    item: &syn::DeriveInput,
    block_size: &Option<usize>,
) -> TokenStream2 {
    match item.data {
        syn::Data::Struct(_) if block_size.is_some() => {
            let block_size = block_size.unwrap();
            quote::quote! {
                fn block_size() -> usize {
                    #block_size
                }
            }
        }
        syn::Data::Struct(_) => {
            quote::quote! {
                std::compile_error!("block for structs require size")
            }
        }
        syn::Data::Enum(ref enum_data) if block_size.is_none() => {
            // build iterafor over enum arms
            // it's either valid tuple of enum::variant => <block_size>
            // or enum::variant => compile_error!(...) to simplify debug
            let enum_arms_iter = enum_data.variants.iter().map(|v| {
                let arm_ident = &v.ident;
                let arm_ident = quote::quote!{ Self::#arm_ident };
                if let syn::Fields::Unnamed(ref field) = v.fields {
                    if field.unnamed.len() == 1 {
                        if let syn::Type::Path(ref type_path) = field.unnamed[0].ty {
                            let type_ident = type_path.path.get_ident();
                            return quote::quote! {
                                #arm_ident(ref v) => <#type_ident as ::block::Block>::iblock_size(v),
                            }
                        }
                    }
                }
                let span = v.ident.span();
                quote::quote_spanned!{ span => _ => {
                    std::compile_error!("only new-type enums with arity of 1 are supported");
                    unimplemented!()
                },}
            });
            let block_size = quote::quote! { <Self as ::block::Block>::block_size() };
            quote::quote! {
                fn block_size() -> usize {
                    4
                }

                fn iblock_size(&self) -> usize {
                    #block_size + match &self {
                        #(#enum_arms_iter)*
                    }
                }
            }
        }
        syn::Data::Union(_) => {
            let span = item.ident.span();
            quote::quote_spanned! { span => std::compiler_error!("unions are not supported") }
        }
        syn::Data::Enum(_) => {
            quote::quote! {
                std::compile_error!("enum blocks should not have size and always u32, due to how serde works with enums");
            }
        }
    }
}

#[proc_macro_attribute]
pub fn block(args: TokenStream, item: TokenStream) -> TokenStream {
    let args = &syn::parse_macro_input!(args as syn::AttributeArgs);
    let item = &syn::parse_macro_input!(item as syn::DeriveInput);

    let block_size = extract_block_size(args);
    let methods = extract_instance_block_size(item, &block_size);

    let ident = &item.ident;
    let block_implementation = quote::quote! {
        impl ::block::Block for #ident {
            #methods
        }
    };

    let mut item = item.to_token_stream();
    item.extend(block_implementation);
    item.into()
}

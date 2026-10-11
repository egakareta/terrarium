use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Fields, parse_macro_input};

#[proc_macro_derive(InstanceProperties, attributes(instance_property))]
pub fn derive_instance_properties(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let fields = match input.data {
        Data::Struct(data) => data.fields,
        _ => {
            return syn::Error::new_spanned(
                input.ident,
                "InstanceProperties can only be derived for structs",
            )
            .to_compile_error()
            .into();
        }
    };

    if matches!(&fields, Fields::Unit) {
        return syn::Error::new_spanned(
            input.ident,
            "InstanceProperties requires a struct with fields",
        )
        .to_compile_error()
        .into();
    }

    let mut properties = Vec::new();
    let mut errors = TokenStream2::new();
    for field in fields {
        for attribute in field
            .attrs
            .iter()
            .filter(|attribute| attribute.path().is_ident("instance_property"))
        {
            match attribute.parse_args::<syn::Ident>() {
                Ok(property) => {
                    let docs = field
                        .attrs
                        .iter()
                        .filter(|attribute| attribute.path().is_ident("doc"));
                    properties.push(quote! {
                        #(#docs)*
                        pub const #property: Self = Self::from_static_name(stringify!(#property));
                    });
                }
                Err(error) => errors.extend(error.to_compile_error()),
            }
        }
    }

    if !errors.is_empty() {
        return errors.into();
    }

    quote! {
        #[allow(non_upper_case_globals)]
        impl crate::InstanceProperty {
            #(#properties)*
        }
    }
    .into()
}

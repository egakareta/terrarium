#![deny(missing_docs)]
#![doc = include_str!("../README.md")]

use proc_macro::TokenStream;
use proc_macro_crate::{FoundCrate, crate_name};
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields, parse_macro_input};

/// Derives field inspection for named structs and enums with unit variants.
#[proc_macro_derive(Inspect, attributes(inspect))]
pub fn inspect(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<proc_macro2::TokenStream> {
    let (package, found) = ["trdevtools", "trrendert", "trscene", "terrarium"]
        .into_iter()
        .find_map(|package| crate_name(package).ok().map(|found| (package, found)))
        .ok_or_else(|| {
            syn::Error::new_spanned(
                &input.ident,
                "Inspect requires a dependency on trdevtools, trrendert, trscene, or terrarium",
            )
        })?;
    let name = match found {
        FoundCrate::Itself => format_ident!("{package}"),
        FoundCrate::Name(name) => format_ident!("{name}"),
    };
    let api = if package == "trdevtools" {
        quote!(::#name)
    } else {
        quote!(::#name::devtools)
    };
    let type_name = input.ident;
    let (impl_generics, type_generics, where_clause) = input.generics.split_for_impl();
    let (read, write, validate) = match input.data {
        Data::Struct(data) => {
            let Fields::Named(fields) = data.fields else {
                return Err(syn::Error::new_spanned(
                    type_name,
                    "Inspect requires named fields",
                ));
            };
            let mut read = Vec::new();
            let mut write = Vec::new();
            let mut validate = Vec::new();
            for field in fields.named {
                let field_name = field.ident.unwrap();
                let key = field_name.to_string();
                let mut skip = false;
                let mut flatten = false;
                let mut readonly = false;
                let mut setter: Option<syn::Ident> = None;
                for attr in &field.attrs {
                    if attr.path().is_ident("inspect") {
                        attr.parse_nested_meta(|meta| {
                            if meta.path.is_ident("skip") {
                                skip = true;
                            } else if meta.path.is_ident("flatten") {
                                flatten = true;
                            } else if meta.path.is_ident("readonly") {
                                readonly = true;
                            } else if meta.path.is_ident("setter") {
                                setter = Some(meta.value()?.parse()?);
                            } else {
                                return Err(
                                    meta.error("expected skip, flatten, readonly, or setter")
                                );
                            }
                            Ok(())
                        })?;
                    }
                }
                if setter.is_some() && (skip || flatten || readonly) {
                    return Err(syn::Error::new_spanned(
                        field_name,
                        "setter cannot be combined with skip, flatten, or readonly",
                    ));
                }
                if skip {
                    continue;
                }
                let cfg: Vec<_> = field
                    .attrs
                    .iter()
                    .filter(|attr| attr.path().is_ident("cfg"))
                    .collect();
                if readonly {
                    read.push(quote! {
                        #(#cfg)*
                        properties.push(#api::Property::readonly(#key, &self.#field_name));
                    });
                } else if flatten {
                    read.push(quote! {
                        #(#cfg)*
                        properties.extend(#api::Inspect::properties(&self.#field_name));
                    });
                    write.push(quote! {
                        #(#cfg)*
                        match #api::Inspect::set_property(&mut self.#field_name, path, value.clone()) {
                            Err(#api::PropertyError::NotFound) => {},
                            result => return result,
                        }
                    });
                    validate.push(quote! {
                        #(#cfg)*
                        match #api::Inspect::validate_property(&self.#field_name, path, value) {
                            Err(#api::PropertyError::NotFound) => {},
                            result => return result,
                        }
                    });
                } else {
                    let assignment = if let Some(setter) = setter {
                        quote! {
                            let mut updated = ::core::clone::Clone::clone(&self.#field_name);
                            #api::Inspect::set_property(&mut updated, tail, value)?;
                            self.#setter(updated);
                            return Ok(());
                        }
                    } else {
                        quote! {
                            return #api::Inspect::set_property(&mut self.#field_name, tail, value);
                        }
                    };
                    read.push(quote! {
                        #(#cfg)*
                        properties.extend(#api::Inspect::properties(&self.#field_name)
                            .into_iter().map(|property| property.prefixed(#key)));
                    });
                    write.push(quote! {
                        #(#cfg)*
                        if let Some(tail) = #api::property_tail(path, #key) {
                            #assignment
                        }
                    });
                    validate.push(quote! {
                        #(#cfg)*
                        if let Some(tail) = #api::property_tail(path, #key) {
                            return #api::Inspect::validate_property(&self.#field_name, tail, value);
                        }
                    });
                }
            }
            (
                quote! { #(#read)* },
                quote! { #(#write)* },
                quote! { #(#validate)* },
            )
        }
        Data::Enum(data) => {
            let mut variants = Vec::new();
            for variant in data.variants {
                if !matches!(variant.fields, Fields::Unit) {
                    return Err(syn::Error::new_spanned(
                        variant,
                        "Inspect requires unit variants",
                    ));
                }
                variants.push(variant.ident);
            }
            (
                quote! {
                    properties.push(#api::Property::new("", #api::PropertyValue::Choice {
                        selected: match self { #(Self::#variants => stringify!(#variants).to_owned()),* },
                        options: vec![#(stringify!(#variants).to_owned()),*],
                    }));
                },
                quote! {
                    if path.is_empty() {
                        if let #api::PropertyValue::Choice { selected, .. } = value {
                            *self = match selected.as_str() {
                                #(stringify!(#variants) => Self::#variants,)*
                                _ => return Err(#api::PropertyError::InvalidValue),
                            };
                            return Ok(());
                        }
                        return Err(#api::PropertyError::InvalidValue);
                    }
                },
                quote! {
                    if path.is_empty() {
                        if let #api::PropertyValue::Choice { selected, .. } = value {
                            if [#(stringify!(#variants)),*].contains(&selected.as_str()) {
                                return Ok(());
                            }
                        }
                        return Err(#api::PropertyError::InvalidValue);
                    }
                },
            )
        }
        _ => {
            return Err(syn::Error::new_spanned(
                type_name,
                "Inspect does not support unions",
            ));
        }
    };
    Ok(quote! {
        impl #impl_generics #api::Inspect for #type_name #type_generics #where_clause {
            fn properties(&self) -> Vec<#api::Property> {
                let mut properties = Vec::new();
                #read
                properties
            }
            fn set_property(&mut self, path: &str, value: #api::PropertyValue)
                -> Result<(), #api::PropertyError> {
                #write
                Err(#api::PropertyError::NotFound)
            }
            fn validate_property(&self, path: &str, value: &#api::PropertyValue)
                -> Result<(), #api::PropertyError> {
                #validate
                Err(#api::PropertyError::NotFound)
            }
        }
    })
}

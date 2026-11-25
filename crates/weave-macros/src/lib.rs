use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Data, DeriveInput, Error, Fields, parse_macro_input};

#[proc_macro_derive(Component)]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand(input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

fn expand(input: DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    if !input.generics.params.is_empty() {
        return Err(Error::new_spanned(
            &input.generics,
            "Component cannot be derived for a generic struct",
        ));
    }
    let fields = match &input.data {
        Data::Struct(data) => match &data.fields {
            Fields::Named(named) => named.named.iter().collect::<Vec<_>>(),
            _ => {
                return Err(Error::new_spanned(
                    name,
                    "Component requires a struct with named fields",
                ));
            }
        },
        _ => {
            return Err(Error::new_spanned(
                name,
                "Component can only be derived for structs",
            ));
        }
    };

    let idents: Vec<_> = fields
        .iter()
        .map(|field| field.ident.clone().expect("named fields have identifiers"))
        .collect();
    let names: Vec<String> = idents.iter().map(|ident| ident.to_string()).collect();
    let types: Vec<_> = fields.iter().map(|field| &field.ty).collect();
    let type_name = name.to_string();

    Ok(quote! {
        impl ::weave_heart::Component for #name {
            const SCHEMA: ::weave_heart::ComponentSchema = ::weave_heart::ComponentSchema::new(
                #type_name,
                &[#(::weave_heart::FieldSchema::new(
                    #names,
                    <#types as ::weave_heart::Field>::KIND,
                )),*],
            );

            fn to_values(&self) -> ::std::vec::Vec<(&'static str, ::weave_heart::Value)> {
                ::std::vec![#((
                    #names,
                    <#types as ::weave_heart::Field>::to_value(&self.#idents),
                )),*]
            }

            fn from_values(
                values: &[(&str, ::weave_heart::Value)],
            ) -> ::core::result::Result<Self, ::weave_heart::DecodeError> {
                ::core::result::Result::Ok(Self {
                    #(#idents: ::weave_heart::take_field::<#types>(values, #names)?,)*
                })
            }
        }
    })
}

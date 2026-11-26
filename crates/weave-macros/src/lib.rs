use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{
    Attribute, Data, DeriveInput, Error, Fields, Ident, Index, LitStr, Member, Type,
    parse_macro_input,
};

#[proc_macro_derive(Component, attributes(component))]
pub fn derive_component(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_component(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

#[proc_macro_derive(Field)]
pub fn derive_field(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    expand_field(&input)
        .unwrap_or_else(|error| error.to_compile_error())
        .into()
}

struct FieldSpec {
    member: Member,
    schema_name: String,
    ty: Type,
    skip: bool,
}

fn check_no_generics(input: &DeriveInput) -> syn::Result<()> {
    if input.generics.params.is_empty() {
        Ok(())
    } else {
        Err(Error::new_spanned(
            &input.generics,
            "the derive cannot be used on a generic type; components and fields must be concrete types",
        ))
    }
}

fn component_name_option(attrs: &[Attribute]) -> syn::Result<Option<String>> {
    let mut name = None;
    for attr in attrs.iter().filter(|a| a.path().is_ident("component")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("name") {
                let value: LitStr = meta.value()?.parse()?;
                name = Some(value.value());
                Ok(())
            } else {
                Err(meta.error("unknown option on the struct; expected `name = \"...\"`"))
            }
        })?;
    }
    Ok(name)
}

fn field_options(attrs: &[Attribute]) -> syn::Result<(Option<String>, bool)> {
    let mut rename = None;
    let mut skip = false;
    for attr in attrs.iter().filter(|a| a.path().is_ident("component")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("rename") {
                let value: LitStr = meta.value()?.parse()?;
                rename = Some(value.value());
                Ok(())
            } else if meta.path.is_ident("skip") {
                skip = true;
                Ok(())
            } else {
                Err(meta.error("unknown field option; expected `rename = \"...\"` or `skip`"))
            }
        })?;
    }
    Ok((rename, skip))
}

fn collect_fields(fields: &Fields) -> syn::Result<Vec<FieldSpec>> {
    let mut specs = Vec::new();
    match fields {
        Fields::Named(named) => {
            for field in &named.named {
                let ident: &Ident = field.ident.as_ref().expect("named fields have identifiers");
                let (rename, skip) = field_options(&field.attrs)?;
                specs.push(FieldSpec {
                    member: Member::Named(ident.clone()),
                    schema_name: rename.unwrap_or_else(|| ident.to_string()),
                    ty: field.ty.clone(),
                    skip,
                });
            }
        }
        Fields::Unnamed(unnamed) => {
            for (index, field) in unnamed.unnamed.iter().enumerate() {
                let (rename, skip) = field_options(&field.attrs)?;
                specs.push(FieldSpec {
                    member: Member::Unnamed(Index::from(index)),
                    schema_name: rename.unwrap_or_else(|| index.to_string()),
                    ty: field.ty.clone(),
                    skip,
                });
            }
        }
        Fields::Unit => {}
    }
    Ok(specs)
}

fn expand_component(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    check_no_generics(input)?;

    let data = match &input.data {
        Data::Struct(data) => data,
        Data::Enum(_) => {
            return Err(Error::new_spanned(
                name,
                "Component can only be derived for structs; use #[derive(Field)] for enums",
            ));
        }
        Data::Union(_) => {
            return Err(Error::new_spanned(
                name,
                "Component cannot be derived for unions",
            ));
        }
    };

    let schema_name = component_name_option(&input.attrs)?.unwrap_or_else(|| name.to_string());
    let specs = collect_fields(&data.fields)?;
    let stored: Vec<&FieldSpec> = specs.iter().filter(|spec| !spec.skip).collect();

    let schema_names: Vec<&str> = stored.iter().map(|s| s.schema_name.as_str()).collect();
    let schema_types: Vec<&Type> = stored.iter().map(|s| &s.ty).collect();
    let stored_members: Vec<&Member> = stored.iter().map(|s| &s.member).collect();

    let constructor = match &data.fields {
        Fields::Named(_) => {
            let members: Vec<&Member> = specs.iter().map(|s| &s.member).collect();
            let values = specs.iter().map(decode_expr);
            quote! { Self { #( #members: #values, )* } }
        }
        Fields::Unnamed(_) => {
            let values = specs.iter().map(decode_expr);
            quote! { Self( #( #values, )* ) }
        }
        Fields::Unit => quote! { Self },
    };

    Ok(quote! {
        impl ::weave_heart::Component for #name {
            const SCHEMA: ::weave_heart::ComponentSchema = ::weave_heart::ComponentSchema::new(
                #schema_name,
                &[ #( ::weave_heart::FieldSchema::new(
                    #schema_names,
                    <#schema_types as ::weave_heart::Field>::KIND,
                ), )* ],
            );

            fn to_values(&self) -> ::std::vec::Vec<(&'static str, ::weave_heart::Value)> {
                ::std::vec![ #( (
                    #schema_names,
                    <#schema_types as ::weave_heart::Field>::to_value(&self.#stored_members),
                ), )* ]
            }

            fn from_values(
                values: &[(&str, ::weave_heart::Value)],
            ) -> ::core::result::Result<Self, ::weave_heart::DecodeError> {
                ::core::result::Result::Ok(#constructor)
            }
        }
    })
}

fn decode_expr(spec: &FieldSpec) -> TokenStream2 {
    if spec.skip {
        return quote! { ::core::default::Default::default() };
    }
    let ty = &spec.ty;
    let schema = &spec.schema_name;
    quote! { ::weave_heart::take_field::<#ty>(values, #schema)? }
}

fn expand_field(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let name = &input.ident;
    check_no_generics(input)?;

    match &input.data {
        Data::Enum(data) => expand_unit_enum(name, data),
        Data::Struct(data) => expand_newtype(name, &data.fields),
        Data::Union(_) => Err(Error::new_spanned(
            name,
            "Field cannot be derived for unions",
        )),
    }
}

fn expand_unit_enum(name: &Ident, data: &syn::DataEnum) -> syn::Result<TokenStream2> {
    if data.variants.is_empty() {
        return Err(Error::new_spanned(
            name,
            "Field cannot be derived for an empty enum",
        ));
    }
    let mut variant_idents = Vec::new();
    for variant in &data.variants {
        if !matches!(variant.fields, Fields::Unit) {
            return Err(Error::new_spanned(
                variant,
                "Field can only be derived for enums whose variants have no data",
            ));
        }
        variant_idents.push(variant.ident.clone());
    }
    let labels: Vec<String> = variant_idents.iter().map(|i| i.to_string()).collect();

    Ok(quote! {
        impl ::weave_heart::Field for #name {
            const KIND: ::weave_heart::FieldKind = ::weave_heart::FieldKind::Text;

            fn to_value(&self) -> ::weave_heart::Value {
                let label: &str = match self {
                    #( Self::#variant_idents => #labels, )*
                };
                ::weave_heart::Value::Text(::std::string::String::from(label))
            }

            fn from_value(value: &::weave_heart::Value) -> ::core::option::Option<Self> {
                match value {
                    ::weave_heart::Value::Text(text) => match text.as_str() {
                        #( #labels => ::core::option::Option::Some(Self::#variant_idents), )*
                        _ => ::core::option::Option::None,
                    },
                    _ => ::core::option::Option::None,
                }
            }
        }
    })
}

fn expand_newtype(name: &Ident, fields: &Fields) -> syn::Result<TokenStream2> {
    let (member, ty) = match fields {
        Fields::Unnamed(unnamed) if unnamed.unnamed.len() == 1 => (
            Member::Unnamed(Index::from(0)),
            unnamed.unnamed[0].ty.clone(),
        ),
        Fields::Named(named) if named.named.len() == 1 => {
            let field = &named.named[0];
            (
                Member::Named(field.ident.clone().expect("named fields have identifiers")),
                field.ty.clone(),
            )
        }
        _ => {
            return Err(Error::new_spanned(
                name,
                "Field can only be derived for enums without data or for structs with exactly one field",
            ));
        }
    };

    let constructor = match fields {
        Fields::Named(_) => quote! { Self { #member: inner } },
        _ => quote! { Self(inner) },
    };

    Ok(quote! {
        impl ::weave_heart::Field for #name {
            const KIND: ::weave_heart::FieldKind = <#ty as ::weave_heart::Field>::KIND;

            fn to_value(&self) -> ::weave_heart::Value {
                <#ty as ::weave_heart::Field>::to_value(&self.#member)
            }

            fn from_value(value: &::weave_heart::Value) -> ::core::option::Option<Self> {
                let inner = <#ty as ::weave_heart::Field>::from_value(value)?;
                ::core::option::Option::Some(#constructor)
            }
        }
    })
}

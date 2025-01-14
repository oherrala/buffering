//! # Structure
//! This crate provides the procedural macro `NoCopy` to the buffering crate. Buffering is feature
//! flagged to be able to use only this macro so there is no reason for this to be used outside of
//! that crate.
//!
//! # Restrictions
//! This only works currently if the following conditions are met for the data type:
//! * It is a struct with named fields
//! * The type of each field is stack allocated
//!
//! # Provided methods
//! Each struct to which `NoCopy` is applied will generate a union type to be used for buffer
//! operations. Traversal works like this:
//! * The union can be initialized either as a struct assigned to the `.structure()` field of the
//! union or using the `MyUnionType::new_buffer()` method and providing a slice
//! * The union also provides methods `.get_field_name` and `.set_field_name` that are
//! generated per struct field
//! * Getters and setters will respect endianness specified by the attribute `#[nocopy_macro(endian =
//! "big")]` or `#[nocopy_macro(endian = "little")]`
//! provided in the original struct
//!
//! # Recognized attributes
//! Attributes can be added to the struct to specify whether integer types should be interpreted
//! as big endian values or little endian values in the form `#[nocopy_macro(endian = "big")]`
//! or `#[nocopy_macro(endian = "little")]`. If neither is specified, native endian is assumed for
//! integers. Another available attribute is provided as `#[nocopy_macro(name = "MyUnionNameHere")]`
//! to override the default name for the autogenerated union.

extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;
use syn::{
    export::Span, DeriveInput, Field, Ident, Lit, Meta, MetaList, MetaNameValue, NestedMeta, Path,
    Type,
};

enum Endian {
    Big,
    Little,
    Default,
}

fn extract_meta(ast: &syn::DeriveInput) -> (Ident, Endian) {
    let mut endian = Endian::Default;
    let mut ident = None;
    for attr in &ast.attrs {
        match attr.style {
            syn::AttrStyle::Outer => (),
            _ => panic!("Only outer attributes allowed here"),
        };
        let ncp_path = &attr.path;
        if ncp_path.get_ident() != Some(&Ident::new("nocopy_macro", Span::call_site())) {
            continue;
        }
        let attrnamemeta = attr.parse_meta();

        match attrnamemeta {
            Ok(Meta::List(MetaList {
                path: _,
                paren_token: _,
                nested,
            })) => {
                for nest in nested.into_iter() {
                    let (path, s) = match nest {
                        NestedMeta::Meta(Meta::NameValue(MetaNameValue {
                            path,
                            eq_token: _,
                            lit: Lit::Str(s),
                        })) => (path, s),
                        _ => panic!("Malformed macro attribute"),
                    };
                    let name_path = syn::parse::<Path>(TokenStream::from(quote! {
                        name
                    }))
                    .expect("Should be a valid path");
                    let endian_path = syn::parse::<Path>(TokenStream::from(quote! {
                        endian
                    }))
                    .expect("Should be a valid path");

                    if path == name_path {
                        let idt = s.value();
                        ident = Some(Ident::new(idt.as_str(), Span::call_site()));
                    }
                    if path == endian_path {
                        endian = match s.value().as_str() {
                            "big" => Endian::Big,
                            "little" => Endian::Little,
                            _ => panic!("Unrecognized \"endian\" option"),
                        }
                    }
                }
            }
            _ => panic!("Outer attribute must be in the form #[nocopy_macro(key = \"value\")]"),
        };
    }
    (
        match ident {
            Some(idt) => idt,
            None => Ident::new(format!("{}Buffer", ast.ident).as_ref(), Span::call_site()),
        },
        endian,
    )
}

fn big_endian(
    ident: &Ident,
    get_ident: &Ident,
    set_ident: &Ident,
    ty: &Type,
) -> quote::__rt::TokenStream {
    quote! {
        pub fn #get_ident(&self) -> #ty {
            unsafe { #ty::from_be(self.structure.#ident) }
        }

        pub fn #set_ident(&mut self, v: #ty) {
            unsafe { self.structure.#ident = v.to_be(); }
        }
    }
}

fn little_endian(
    ident: &Ident,
    get_ident: &Ident,
    set_ident: &Ident,
    ty: &Type,
) -> quote::__rt::TokenStream {
    quote! {
        pub fn #get_ident(&self) -> #ty {
            unsafe { #ty::from_le(self.structure.#ident) }
        }

        pub fn #set_ident(&mut self, v: #ty) {
            unsafe { self.structure.#ident = v.to_le(); }
        }
    }
}

fn native_endian(
    ident: &Ident,
    get_ident: &Ident,
    set_ident: &Ident,
    ty: &Type,
) -> quote::__rt::TokenStream {
    quote! {
        pub fn #get_ident(&self) -> #ty {
            unsafe { self.structure.#ident }
        }

        pub fn #set_ident(&mut self, v: #ty) {
            unsafe { self.structure.#ident = v; }
        }
    }
}

fn match_endian(named_field: &Field, endian: &Endian) -> quote::__rt::TokenStream {
    let ident = match named_field.ident {
        Some(ref idt) => idt,
        None => panic!("All struct fields must be named"),
    };
    let get_ident = Ident::new(
        format!(
            "get_{}",
            named_field
                .ident
                .as_ref()
                .expect("All fields must be named")
        )
        .as_str(),
        Span::call_site(),
    );
    let set_ident = Ident::new(
        format!(
            "set_{}",
            named_field
                .ident
                .as_ref()
                .expect("All fields must be named")
        )
        .as_str(),
        Span::call_site(),
    );
    let ty = &named_field.ty;

    let u8_ty = syn::parse::<Type>(TokenStream::from(quote! {
        u8
    }))
    .expect("Should be a valid type");
    let u16_ty = syn::parse::<Type>(TokenStream::from(quote! {
        u16
    }))
    .expect("Should be a valid type");
    let u32_ty = syn::parse::<Type>(TokenStream::from(quote! {
        u32
    }))
    .expect("Should be a valid type");
    let u64_ty = syn::parse::<Type>(TokenStream::from(quote! {
        u64
    }))
    .expect("Should be a valid type");

    if *ty == u8_ty || *ty == u16_ty || *ty == u32_ty || *ty == u64_ty {
        match endian {
            Endian::Big => big_endian(&ident, &get_ident, &set_ident, ty),
            Endian::Little => little_endian(&ident, &get_ident, &set_ident, ty),
            Endian::Default => native_endian(&ident, &get_ident, &set_ident, ty),
        }
    } else {
        native_endian(&ident, &get_ident, &set_ident, ty)
    }
}

/// Procedural macro that will derive getters and setters with appropriate endianness for every
/// field defined in the struct
#[proc_macro_derive(NoCopy, attributes(nocopy_macro))]
pub fn no_copy(input: TokenStream) -> TokenStream {
    let ast: DeriveInput = syn::parse(input).expect("Failed to parse input");

    if ast
        .attrs
        .iter()
        .filter(|item| {
            item.parse_meta().expect("Provided attribute not valid")
                == syn::parse::<Meta>(TokenStream::from(quote! {
                    repr(C)
                }))
                .expect("Should be a valid attribute")
        })
        .collect::<Vec<_>>()
        .len()
        < 1
    {
        panic!("Struct must be marked as #[repr(C)] to be used with this derive")
    }

    let name = &ast.ident;
    let (attrname, endian) = extract_meta(&ast);

    let fields = match ast.data {
        syn::Data::Struct(structure) => structure.fields,
        _ => panic!("This macro only supports structs"),
    };
    let field_pairs = match fields {
        syn::Fields::Named(named) => named.named,
        _ => panic!("This macro only supports structs with named fields"),
    };

    let mut funcs_vec = Vec::new();
    for named_field in field_pairs {
        funcs_vec.push(match_endian(&named_field, &endian));
    }

    TokenStream::from(quote! {
        #[derive(Copy,Clone)]
        #[repr(C)]
        pub union #attrname {
            structure: #name,
            buffer: [u8; std::mem::size_of::<#name>()]
        }

        impl #attrname {
            pub fn new_buffer(buffer: [u8; std::mem::size_of::<#name>()]) -> Self {
                #attrname { buffer }
            }

            pub fn as_buffer(&self) -> &[u8] {
                unsafe { &self.buffer }
            }

            #(
                #funcs_vec
            )*
        }
    })
}

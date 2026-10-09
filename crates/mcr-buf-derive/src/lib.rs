use proc_macro::TokenStream;
use proc_macro2::{Literal, TokenStream as Tokens};
use quote::{format_ident, quote};
use syn::{
    Data, DeriveInput, Expr, ExprLit, ExprUnary, Fields, GenericParam, Ident, Lit, LitInt, Path,
    UnOp, Variant, parse_macro_input, parse_quote, spanned::Spanned,
};

#[proc_macro_derive(Encode, attributes(mcr))]
pub fn derive_encode(input: TokenStream) -> TokenStream {
    expand(input, encode_impl)
}

#[proc_macro_derive(Decode, attributes(mcr))]
pub fn derive_decode(input: TokenStream) -> TokenStream {
    expand(input, decode_impl)
}

fn expand(input: TokenStream, f: fn(&DeriveInput) -> syn::Result<Tokens>) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    f(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

enum Kind {
    Plain,
    VarInt,
    VarLong,
    With(Path),
    MaxLen(LitInt),
}

fn field_kind(field: &syn::Field) -> syn::Result<Kind> {
    let mut kind = Kind::Plain;
    for attr in field.attrs.iter().filter(|a| a.path().is_ident("mcr")) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("varint") {
                kind = Kind::VarInt;
            } else if meta.path.is_ident("varlong") {
                kind = Kind::VarLong;
            } else if meta.path.is_ident("with") {
                kind = Kind::With(meta.value()?.parse()?);
            } else if meta.path.is_ident("max_len") {
                kind = Kind::MaxLen(meta.value()?.parse()?);
            } else {
                return Err(meta.error("unknown mcr field attribute"));
            }
            Ok(())
        })?;
    }
    Ok(kind)
}

#[derive(Clone, Copy)]
enum Tag {
    VarInt,
    Prim(&'static str),
}

fn container_tag(input: &DeriveInput) -> syn::Result<Tag> {
    let mut tag = Tag::VarInt;
    for attr in input.attrs.iter().filter(|a| a.path().is_ident("mcr")) {
        attr.parse_nested_meta(|meta| {
            if !meta.path.is_ident("tag") {
                return Err(meta.error("unknown mcr container attribute"));
            }
            let ty: Ident = meta.value()?.parse()?;
            tag = match ty.to_string().as_str() {
                "varint" => Tag::VarInt,
                "u8" => Tag::Prim("u8"),
                "i8" => Tag::Prim("i8"),
                "u16" => Tag::Prim("u16"),
                "i16" => Tag::Prim("i16"),
                "i32" => Tag::Prim("i32"),
                "i64" => Tag::Prim("i64"),
                _ => {
                    return Err(syn::Error::new(
                        ty.span(),
                        "tag must be u8, i8, u16, i16, i32, i64 or varint",
                    ));
                }
            };
            Ok(())
        })?;
    }
    Ok(tag)
}

fn bindings(fields: &Fields) -> Vec<Ident> {
    (0..fields.len()).map(|i| format_ident!("f{i}")).collect()
}

fn pattern(path: &Tokens, fields: &Fields) -> Tokens {
    let b = bindings(fields);
    match fields {
        Fields::Named(n) => {
            let names = n.named.iter().map(|f| f.ident.as_ref().unwrap());
            quote!(#path { #(#names: #b),* })
        }
        Fields::Unnamed(_) => quote!(#path(#(#b),*)),
        Fields::Unit => quote!(#path),
    }
}

fn encode_fields(fields: &Fields) -> syn::Result<Tokens> {
    let mut out = Tokens::new();
    for (field, b) in fields.iter().zip(bindings(fields)) {
        out.extend(match field_kind(field)? {
            Kind::Plain | Kind::MaxLen(_) => quote!(::mcr_buf::Encode::encode(#b, out);),
            Kind::VarInt => quote!(::mcr_buf::write_varint(out, *#b);),
            Kind::VarLong => quote!(::mcr_buf::write_varlong(out, *#b);),
            Kind::With(p) => quote!(#p::encode(#b, out);),
        });
    }
    Ok(out)
}

fn decode_fields(path: &Tokens, fields: &Fields) -> syn::Result<Tokens> {
    let mut values = Vec::new();
    for field in fields {
        values.push(match field_kind(field)? {
            Kind::Plain => quote!(::mcr_buf::Decode::decode(input)?),
            Kind::VarInt => quote!(::mcr_buf::read_varint(input)?),
            Kind::VarLong => quote!(::mcr_buf::read_varlong(input)?),
            Kind::With(p) => quote!(#p::decode(input)?),
            Kind::MaxLen(n) => quote!(::mcr_buf::__private::StrField::decode_field(input, #n)?),
        });
    }
    Ok(match fields {
        Fields::Named(n) => {
            let names = n.named.iter().map(|f| f.ident.as_ref().unwrap());
            quote!(#path { #(#names: #values),* })
        }
        Fields::Unnamed(_) => quote!(#path(#(#values),*)),
        Fields::Unit => quote!(#path),
    })
}

fn discriminants(variants: &[&Variant]) -> syn::Result<Vec<i64>> {
    let mut next = 0i64;
    let mut seen = Vec::new();
    for v in variants {
        if let Some((_, expr)) = &v.discriminant {
            next = literal_i64(expr)?;
        }
        if seen.contains(&next) {
            return Err(syn::Error::new(v.span(), "duplicate discriminant"));
        }
        seen.push(next);
        next += 1;
    }
    Ok(seen)
}

fn literal_i64(expr: &Expr) -> syn::Result<i64> {
    match expr {
        Expr::Lit(ExprLit {
            lit: Lit::Int(i), ..
        }) => i.base10_parse(),
        Expr::Unary(ExprUnary {
            op: UnOp::Neg(_),
            expr,
            ..
        }) => Ok(-literal_i64(expr)?),
        _ => Err(syn::Error::new(
            expr.span(),
            "discriminant must be an integer literal",
        )),
    }
}

fn encode_tag(tag: Tag, value: i64) -> Tokens {
    let lit = Literal::i64_unsuffixed(value);
    match tag {
        Tag::VarInt => quote!(::mcr_buf::write_varint(out, #lit as i32);),
        Tag::Prim(ty) => {
            let ty = format_ident!("{ty}");
            quote!(::mcr_buf::Encode::encode(&(#lit as #ty), out);)
        }
    }
}

fn decode_tag(tag: Tag) -> Tokens {
    match tag {
        Tag::VarInt => quote!(::mcr_buf::read_varint(input)? as i64),
        Tag::Prim(ty) => {
            let ty = format_ident!("{ty}");
            quote!(<#ty as ::mcr_buf::Decode>::decode(input)? as i64)
        }
    }
}

fn encode_impl(input: &DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    for p in generics.type_params_mut() {
        p.bounds.push(parse_quote!(::mcr_buf::Encode));
    }
    let (impl_g, _, where_c) = generics.split_for_impl();
    let (_, ty_g, _) = input.generics.split_for_impl();

    let body = match &input.data {
        Data::Struct(s) => {
            let pat = pattern(&quote!(Self), &s.fields);
            let enc = encode_fields(&s.fields)?;
            quote!(let #pat = self; #enc)
        }
        Data::Enum(e) => {
            let tag = container_tag(input)?;
            let variants: Vec<_> = e.variants.iter().collect();
            let discs = discriminants(&variants)?;
            let mut arms = Vec::new();
            for (v, d) in variants.iter().zip(discs) {
                let vn = &v.ident;
                let pat = pattern(&quote!(Self::#vn), &v.fields);
                let tag = encode_tag(tag, d);
                let enc = encode_fields(&v.fields)?;
                arms.push(quote!(#pat => { #tag #enc }));
            }
            quote!(match self { #(#arms)* })
        }
        Data::Union(_) => return Err(syn::Error::new(input.span(), "unions are not supported")),
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_g ::mcr_buf::Encode for #name #ty_g #where_c {
            fn encode(&self, out: &mut ::std::vec::Vec<u8>) {
                #body
            }
        }
    })
}

fn decode_impl(input: &DeriveInput) -> syn::Result<Tokens> {
    let name = &input.ident;
    let mut generics = input.generics.clone();
    let lt = match input.generics.lifetimes().next() {
        Some(l) => l.lifetime.clone(),
        None => {
            let lt: syn::Lifetime = parse_quote!('__de);
            generics
                .params
                .insert(0, GenericParam::Lifetime(parse_quote!(#lt)));
            lt
        }
    };
    for p in generics.type_params_mut() {
        p.bounds.push(parse_quote!(::mcr_buf::Decode<#lt>));
    }
    let (impl_g, _, where_c) = generics.split_for_impl();
    let (_, ty_g, _) = input.generics.split_for_impl();

    let body = match &input.data {
        Data::Struct(s) => {
            let v = decode_fields(&quote!(Self), &s.fields)?;
            quote!(::std::result::Result::Ok(#v))
        }
        Data::Enum(e) => {
            let tag = container_tag(input)?;
            let variants: Vec<_> = e.variants.iter().collect();
            let discs = discriminants(&variants)?;
            let mut arms = Vec::new();
            for (v, d) in variants.iter().zip(discs) {
                let vn = &v.ident;
                let lit = Literal::i64_unsuffixed(d);
                let value = decode_fields(&quote!(Self::#vn), &v.fields)?;
                arms.push(quote!(#lit => ::std::result::Result::Ok(#value),));
            }
            let read = decode_tag(tag);
            let what = name.to_string();
            quote! {
                let raw: i64 = #read;
                match raw {
                    #(#arms)*
                    _ => ::std::result::Result::Err(::mcr_buf::DecodeError::InvalidValue {
                        what: #what,
                        value: raw,
                    }),
                }
            }
        }
        Data::Union(_) => return Err(syn::Error::new(input.span(), "unions are not supported")),
    };

    Ok(quote! {
        #[automatically_derived]
        impl #impl_g ::mcr_buf::Decode<#lt> for #name #ty_g #where_c {
            fn decode(input: &mut &#lt [u8]) -> ::mcr_buf::Result<Self> {
                #body
            }
        }
    })
}

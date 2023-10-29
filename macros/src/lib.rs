#![feature(proc_macro_diagnostic)]

use proc_macro2::Span;
use quote::quote;
use syn::{
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Attribute, Generics,
};

fn unimplemented_call() -> syn::Stmt {
    let a = quote! {
        unimplemented!();
    };

    syn::parse2(a).unwrap()
}

struct ProtoFn {
    attrs: Vec<syn::Attribute>,
    _externality: syn::Token![extern],
    fn_token: syn::Token![fn],
    ident: syn::Ident,
    _paren: syn::token::Paren,
    inputs: Punctuated<syn::Type, syn::Token![,]>,
    output: syn::ReturnType,
}

impl Parse for ProtoFn {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let inner;

        Ok(Self {
            attrs: input.call(Attribute::parse_outer)?,
            _externality: input.parse()?,
            fn_token: input.parse()?,
            ident: input.parse()?,
            _paren: syn::parenthesized!(inner in input),
            inputs: Punctuated::parse_terminated(&inner)?,
            output: input.parse()?,
        })
    }
}

///
/// Delcares a new method in our API.
///
/// In the parenthesis, only put the parameter's types (no identifiers).
///
/// Syntax:
/// ```ignore
/// use avsocket::declare;
///
/// declare!(
///     ///
///     /// My cool new function.
///     ///
///     extern fn cool() -> String
/// );
///
/// declare!(
///     ///
///     /// This one's even cooler!
///     ///
///     extern fn even_cooler(usize) -> String
/// );
/// ```
///
/// Note: all inputs and outputs **must** implement `serde::Serialize`, `serde::DeserializeOwned`.
///
#[proc_macro]
pub fn declare(func: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let sig: ProtoFn = parse_macro_input!(func);

    // Make type for the Method<(...,), ...>

    let input_ty = sig.inputs.iter().cloned();

    let tuple = syn::Type::Tuple(syn::TypeTuple {
        paren_token: Default::default(),
        elems: Punctuated::from_iter(input_ty),
    });

    let internal_ident = syn::Ident::new(&format!("_{}", sig.ident), Span::call_site());

    // Manually add () if return type missing;
    let const_output = match &sig.output {
        syn::ReturnType::Default => syn::Type::Tuple(syn::TypeTuple {
            paren_token: Default::default(),
            elems: Punctuated::new(),
        }),
        syn::ReturnType::Type(_, ty) => *(ty.clone()),
    };

    let ident = sig.ident;
    let ident_str = syn::LitStr::new(ident.to_string().as_str(), ident.span());
    let attrs = sig.attrs;
    let method_decl = quote! {
        #(#attrs)*
        #[allow(non_upper_case_globals)]
        pub const #ident: ::avsocket::Method<#tuple, #const_output> = ::avsocket::methodify(&#internal_ident, #ident_str);
    };

    let method_decl = syn::parse2::<syn::ItemConst>(method_decl).expect("Valid method decl.");

    // Make Internal (_) function
    let internal_func = syn::Signature {
        constness: None,
        asyncness: None,
        unsafety: None,
        abi: None,
        fn_token: sig.fn_token,
        ident: internal_ident,
        generics: Generics {
            lt_token: None,
            params: Punctuated::new(),
            gt_token: None,
            where_clause: None,
        },
        paren_token: Default::default(),
        inputs: Punctuated::from_iter(sig.inputs.into_iter().map(|ty| {
            syn::FnArg::Typed(syn::PatType {
                attrs: Default::default(),
                pat: Box::new(syn::Pat::Wild(syn::PatWild {
                    attrs: Default::default(),
                    underscore_token: Default::default(),
                })),
                colon_token: Default::default(),
                ty: Box::new(ty.clone()),
            })
        })),
        variadic: None,
        output: sig.output,
    };

    let internal_func = syn::ItemFn {
        attrs: Default::default(),
        vis: syn::Visibility::Inherited,
        sig: internal_func,
        block: Box::new(syn::Block {
            brace_token: Default::default(),
            stmts: vec![unimplemented_call()],
        }),
    };

    quote! {
        #internal_func
        #method_decl
    }
    .into()
}

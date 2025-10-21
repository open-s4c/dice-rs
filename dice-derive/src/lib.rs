use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, Fields, Item, parse_macro_input, spanned::Spanned};

/// Attribute macro:
/// Usage:
///     #[dice_event(<event-id>)]
///     struct Foo;
#[proc_macro_attribute]
pub fn dice_event(attr: TokenStream, item: TokenStream) -> TokenStream {
    let id_expr = parse_macro_input!(attr as Expr);

    let input = parse_macro_input!(item as Item);

    let (ident, generics, _where_clause, is_unit) = match &input {
        Item::Struct(s) => {
            let ident = s.ident.clone();
            let generics = s.generics.clone();
            let (_, _, where_clause) = generics.split_for_impl();
            let is_unit = matches!(s.fields, Fields::Unit);
            (ident, s.generics.clone(), where_clause.cloned(), is_unit)
        }
        _ => {
            let err = syn::Error::new(
                input.span(),
                "#[dice_event(...)] can only be used on structs",
            );
            return err.to_compile_error().into();
        }
    };

    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();

    let impl_tokens = if is_unit {
        quote! {
            impl #impl_generics DiceEvent for #ident #ty_generics #where_clause {
                const ID: TypeId = (#id_expr) as TypeId;

                fn fallback<'a>() -> ::core::option::Option<&'a Self> {
                    ::core::option::Option::Some(&Self)
                }
            }
        }
    } else {
        quote! {
            impl #impl_generics DiceEvent for #ident #ty_generics #where_clause {
                const ID: TypeId = (#id_expr) as TypeId;
            }
        }
    };

    TokenStream::from(quote! {
        #input
        #impl_tokens
    })
}

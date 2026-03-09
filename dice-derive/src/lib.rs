//! Procedural macros for the dice-rs crate.
//!
//! This crate provides the [`dice_event`] attribute macro for implementing
//! the `DiceEvent` trait on event structs.

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, spanned::Spanned, Expr, Fields, Item};

/// Derive the `DiceEvent` trait for an event struct.
///
/// This attribute macro implements `DiceEvent` for the annotated struct,
/// setting its `ID` constant to the provided event type ID.
///
/// # Usage
///
/// ```ignore
/// use dice_rs::{DiceEvent, TypeId};
/// use dice_derive::dice_event;
///
/// // For unit structs (marker events with no payload):
/// #[repr(C)]
/// #[derive(Copy, Clone, Debug)]
/// #[dice_event(EVENT_THREAD_START)]
/// pub struct ThreadStartEvent;
///
/// // For structs with fields:
/// #[repr(C)]
/// #[derive(Copy, Clone, Debug)]
/// #[dice_event(EVENT_MALLOC)]
/// pub struct MallocEvent {
///     pub size: usize,
///     pub ret: *const (),
/// }
/// ```
///
/// # Behavior
///
/// - **Unit structs**: The macro implements `fallback()` to return `Some(&Self)`,
///   allowing dice to pass null pointers for marker events. This is safe because
///   unit structs have no fields to read.
///
/// - **Structs with fields**: The macro uses the default `fallback()` implementation
///   which returns `None`. If dice passes a null pointer for such an event,
///   `from_raw` will return `None`.
///
/// # Requirements
///
/// The struct should be marked `#[repr(C)]` to ensure memory layout compatibility
/// with dice's C event structures.
///
/// # Parameters
///
/// - `event_id`: An expression that evaluates to a `TypeId` (typically a constant
///   from `dice_rs::events::raw`, e.g., `raw::EVENT_MALLOC`).
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

                fn fallback_const<'a>() -> ::core::option::Option<&'a Self> {
                    ::core::option::Option::None
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

use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use syn::{Ident, Result};

impl StateMachine {
    pub(super) fn callback_filter_expr(
        &self,
        callback: &TransitionCallback,
        state_ident: &Ident,
        event_ident: &Ident,
        current_expr: &TokenStream2,
        target_expr: &TokenStream2,
    ) -> Result<TokenStream2> {
        let leaves = &self.states;
        let callback_name = callback.name.to_string();

        let expand_filter = |idents: &[Ident], label: &str| -> Result<Vec<Ident>> {
            let mut expanded = Vec::new();
            let mut seen = std::collections::HashSet::new();

            for ident in idents {
                let resolved = self.hierarchy.expand_state(ident, leaves);
                if resolved.is_empty() {
                    return Err(syn::Error::new(
                        ident.span(),
                        format!(
                            "unknown state `{}` in `{}` filter of callback `{}`",
                            ident, label, callback_name
                        ),
                    ));
                }

                for leaf in resolved {
                    let key = leaf.to_string();
                    if seen.insert(key) {
                        expanded.push(leaf);
                    }
                }
            }

            Ok(expanded)
        };

        let from_tokens = if callback.from.is_empty() {
            quote!(true)
        } else {
            let expanded = expand_filter(&callback.from, "from")?;
            if expanded.len() == 1 {
                let state = &expanded[0];
                quote!(#current_expr == #state_ident::#state)
            } else {
                let patterns = expanded.iter().map(|state| quote!(#state_ident::#state));
                quote!(matches!(#current_expr, #( #patterns )|*))
            }
        };

        let to_tokens = if callback.to.is_empty() {
            quote!(true)
        } else {
            let expanded = expand_filter(&callback.to, "to")?;
            if expanded.len() == 1 {
                let state = &expanded[0];
                quote!(#target_expr == #state_ident::#state)
            } else {
                let patterns = expanded.iter().map(|state| quote!(#state_ident::#state));
                quote!(matches!(#target_expr, #( #patterns )|*))
            }
        };

        let event_expr = if callback.on.is_empty() {
            quote!(true)
        } else {
            let names = callback.on.iter().map(|event| quote!(stringify!(#event)));
            quote!({ let event_name = stringify!(#event_ident); #( event_name == #names )||* })
        };

        Ok(quote!(#from_tokens && #to_tokens && #event_expr))
    }
}

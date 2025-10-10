use crate::types::*;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::Ident;

pub(super) struct TransitionStorageOps {
    pub(super) pre_state: TokenStream2,
    pub(super) post_state: TokenStream2,
    pub(super) rollback: TokenStream2,
}

impl StateMachine {
    pub(super) fn build_transition_storage_ops(
        &self,
        _state_ident: &Ident,
        source: &Ident,
        target: &Ident,
    ) -> TransitionStorageOps {
        let mut pre_ops = Vec::new();
        let mut post_ops = Vec::new();
        let mut rollback_ops = Vec::new();

        for spec in &self.state_storage {
            let field = &spec.field;
            let backup_ident = format_ident!("__storage_backup_{}", field);

            if !spec.is_superstate {
                let affects_source = &spec.owner == source;
                let affects_target = &spec.owner == target;

                if !(affects_source || affects_target) {
                    continue;
                }

                pre_ops.push(quote! {
                    let #backup_ident = self.#field.take();
                });

                if affects_target {
                    post_ops.push(quote! {
                        self.#field =
                            ::core::option::Option::Some(::core::default::Default::default());
                    });
                }

                rollback_ops.push(quote! {
                    self.#field = #backup_ident;
                });
            } else if let Some(descendants) = self.hierarchy.lookup.get(&spec.owner.to_string()) {
                let source_in = descendants.iter().any(|leaf| leaf == source);
                let target_in = descendants.iter().any(|leaf| leaf == target);

                if source_in == target_in {
                    continue;
                }

                pre_ops.push(quote! {
                    let #backup_ident = self.#field.take();
                });

                if !source_in && target_in {
                    post_ops.push(quote! {
                        self.#field =
                            ::core::option::Option::Some(::core::default::Default::default());
                    });
                }

                rollback_ops.push(quote! {
                    self.#field = #backup_ident;
                });
            }
        }

        TransitionStorageOps {
            pre_state: quote! { #( #pre_ops )* },
            post_state: quote! { #( #post_ops )* },
            rollback: quote! { #( #rollback_ops )* },
        }
    }
}

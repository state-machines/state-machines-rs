//! Eventless microsteps select guards once, then call an unchecked edge helper.
//! A budget prevents unbounded automatic cycles without poisoning committed state.
use super::utils::{maybe_async, maybe_await};
use crate::types::StateMachine;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};

pub fn dynamic_methods(machine: &StateMachine) -> TokenStream {
    let any = format_ident!("Any{}State", machine.name);
    let async_ = maybe_async(machine.async_mode);
    let await_ = maybe_await(machine.async_mode);
    let error_ty = machine.error.as_ref().map_or(
        quote! { ::state_machines::DynamicError },
        |ty| quote! { ::state_machines::DynamicError<#ty> },
    );
    let error_ctor = machine.error.as_ref().map_or(
        quote! { ::state_machines::DynamicError },
        |ty| quote! { ::state_machines::DynamicError::<#ty> },
    );
    let map_error = if machine.error.is_some() {
        quote! { ::state_machines::DynamicError::from_event_error(error) }
    } else {
        quote! { ::state_machines::DynamicError::from_guard_error(error) }
    };
    let step_arms = machine.states.iter().map(|source| {
        let groups = super::branching::groups(machine, source);
        let selections = groups.iter().filter_map(|edges| {
            let event = machine
                .events
                .iter()
                .find(|event| event.name == edges[0].event)
                .unwrap();
            if !event.automatic {
                return None;
            }
            let event_enabled = super::branching::condition_on(
                machine,
                &event.hooks,
                &quote! {},
                &quote! { current },
            );
            let choices = edges.iter().enumerate().map(|(index, edge)| {
                let enabled = super::branching::condition_on(
                    machine,
                    &edge.selection,
                    &quote! {},
                    &quote! { current },
                );
                let history = super::history::condition_on(edge, &quote! { current });
                let helper = if edges.len() > 1 {
                    format_ident!("__sm_{}_{}", event.name, index)
                } else {
                    format_ident!("__sm_auto_{}", event.name)
                };
                let target = &edge.target;
                let external = !edge.internal;
                quote! {
                    if #history && (#enabled) {
                        match current.#helper() #await_ {
                            Ok(machine) => {
                                if #external { self.epoch = self.epoch.wrapping_add(1); }
                                let next = #any::#target(machine);
                                self.last_state = next.state();
                                self.inner = Some(next);
                                self.completions.extend_from_slice(self.completion_events());
                                return Ok(true);
                            }
                            Err((machine, error)) => {
                                self.inner = Some(#any::#source(machine));
                                return Err(#map_error);
                            }
                        }
                    }
                }
            });
            Some(quote! { if #event_enabled { #( #choices )* } })
        });
        quote! {
            #any::#source(current) => {
                #( #selections )*
                self.inner = Some(#any::#source(current));
                Ok(false)
            }
        }
    });
    let available_arms = machine.states.iter().map(|state| {
        let checks = super::branching::groups(machine, state)
            .iter()
            .filter_map(|edges| {
                let event = machine
                    .events
                    .iter()
                    .find(|event| event.name == edges[0].event)
                    .unwrap();
                event.automatic.then(|| {
                    let can = format_ident!("can_{}", event.name);
                    quote! { current.#can() #await_ }
                })
            })
            .collect::<Vec<_>>();
        if checks.is_empty() {
            quote! { #any::#state(_) => false }
        } else {
            quote! { #any::#state(current) => false #( || #checks )* }
        }
    });
    quote! {
        #async_ fn __sm_has_automatic(&self) -> bool {
            match self.inner.as_ref() {
                Some(inner) => match inner { #( #available_arms, )* },
                None => false,
            }
        }
        #async_ fn __sm_automatic_step(&mut self) -> Result<bool, #error_ty> {
            let current = self.inner.take().ok_or(#error_ctor::Poisoned {
                from: self.last_state.name(), event: "__automatic",
            })?;
            match current { #( #step_arms, )* }
        }
        /// Run enabled eventless transitions to stability, bounded by `max_steps`.
        /// Failure retains the last committed state; a limit is not poisoning.
        pub #async_ fn stabilize(&mut self, max_steps: usize) -> Result<usize, #error_ty> {
            if self.is_poisoned() {
                return Err(#error_ctor::Poisoned { from: self.last_state.name(), event: "__automatic" });
            }
            let mut steps = 0;
            loop {
                if steps == max_steps {
                    return if self.__sm_has_automatic() #await_ {
                        Err(#error_ctor::StepLimit { limit: max_steps })
                    } else { Ok(steps) };
                }
                if !(self.__sm_automatic_step() #await_?) { return Ok(steps); }
                steps += 1;
            }
        }
    }
}

//! History remembers control state, not suspended data. Its optional Copy
//! fields add no allocation and are updated only after a successful exit.

use crate::types::{HistoryChoice, HistoryMode, StateMachine, Transition, TransitionEdge};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::Ident;

pub fn regions(machine: &StateMachine) -> Vec<&Ident> {
    let mut regions = Vec::new();
    for transition in machine.events.iter().flat_map(|event| &event.transitions) {
        if transition.history.is_some() && !regions.contains(&&transition.target) {
            regions.push(&transition.target);
        }
    }
    regions
}

pub fn field(region: &Ident) -> Ident {
    format_ident!(
        "__sm_history_{}",
        super::utils::to_snake_case(&region.to_string())
    )
}

pub fn fields(machine: &StateMachine) -> Vec<Ident> {
    regions(machine).into_iter().map(field).collect()
}

fn shallow_target(machine: &StateMachine, region: &Ident, leaf: &Ident) -> Ident {
    if let Some(path) = machine.hierarchy.ancestors.get(&leaf.to_string())
        && let Some(index) = path.iter().position(|ancestor| ancestor == region)
        && let Some(child) = path.get(index + 1)
    {
        return machine
            .hierarchy
            .resolve_target(child)
            .unwrap_or_else(|| leaf.clone());
    }
    leaf.clone()
}

pub fn choices(
    machine: &StateMachine,
    transition: &Transition,
) -> Vec<(Ident, Option<HistoryChoice>)> {
    let initial = machine
        .hierarchy
        .resolve_target(&transition.target)
        .unwrap_or_else(|| transition.target.clone());
    let Some(mode) = transition.history else {
        return vec![(initial, None)];
    };
    let region = &transition.target;
    let mut choices = vec![(
        initial,
        Some(HistoryChoice {
            region: region.clone(),
            stored: vec![None],
        }),
    )];
    for (index, leaf) in machine.states.iter().enumerate() {
        if !machine
            .hierarchy
            .expand_state(region, &machine.states)
            .contains(leaf)
        {
            continue;
        }
        let target = if mode == HistoryMode::Deep {
            leaf.clone()
        } else {
            shallow_target(machine, region, leaf)
        };
        if let Some((_, Some(choice))) = choices
            .iter_mut()
            .find(|(candidate, _)| candidate == &target)
        {
            choice.stored.push(Some(index));
        } else {
            choices.push((
                target,
                Some(HistoryChoice {
                    region: region.clone(),
                    stored: vec![Some(index)],
                }),
            ));
        }
    }
    choices
}

pub fn condition(edge: &TransitionEdge) -> TokenStream {
    let Some(choice) = &edge.history else {
        return quote! { true };
    };
    let field = field(&choice.region);
    let tests = choice.stored.iter().map(|index| match index {
        Some(index) => quote! { self.#field == Some(#index) },
        None => quote! { self.#field.is_none() },
    });
    quote! { (false #( || #tests )*) }
}

pub fn record_exit(machine: &StateMachine, source: &Ident, edge: &TransitionEdge) -> TokenStream {
    if edge.internal {
        return quote! {};
    }
    let index = machine
        .states
        .iter()
        .position(|state| state == source)
        .unwrap();
    let updates = regions(machine)
        .into_iter()
        .filter(|region| {
            let leaves = machine.hierarchy.expand_state(region, &machine.states);
            leaves.contains(source)
                && (!leaves.contains(&edge.target)
                    || machine.reentered_superstate(source, edge, region))
        })
        .map(|region| {
            let field = field(region);
            quote! { new_machine.#field = Some(#index); }
        });
    quote! { #( #updates )* }
}

//! Named orthogonal declarations compile into the existing Parallel/Runner driver.
use crate::codegen::utils::to_pascal_case;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashSet;
use syn::{
    Expr, Ident, Result, Token, Type, braced,
    parse::{Parse, ParseStream},
};

pub struct Regions {
    name: Ident,
    regions: Vec<(Ident, Type)>,
    events: Vec<Event>,
    snapshot: bool,
}
struct Event {
    name: Ident,
    payload: Option<Type>,
    routes: Vec<(Ident, Expr)>,
}

/// Both region type declarations and owned event routes use the same map grammar.
fn parse_named<T: Parse>(input: ParseStream<'_>) -> Result<Vec<(Ident, T)>> {
    let content;
    braced!(content in input);
    let mut values = Vec::new();
    while !content.is_empty() {
        let name = content.parse()?;
        content.parse::<Token![:]>()?;
        values.push((name, content.parse()?));
        if !content.is_empty() {
            content.parse::<Token![,]>()?;
        }
    }
    Ok(values)
}

impl Parse for Regions {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let mut name = None;
        let mut regions = Vec::new();
        let mut events = Vec::new();
        let mut snapshot = false;
        let mut fields = HashSet::new();
        while !input.is_empty() {
            let key: Ident = input.parse()?;
            if !fields.insert(key.to_string()) {
                return Err(syn::Error::new(
                    key.span(),
                    "duplicate region machine field",
                ));
            }
            if input.peek(Token![:]) {
                input.parse::<Token![:]>()?;
            }
            match key.to_string().as_str() {
                "name" => name = Some(input.parse()?),
                "snapshot" => snapshot = input.parse::<syn::LitBool>()?.value,
                "regions" => regions = parse_named(input)?,
                "events" => {
                    let content;
                    braced!(content in input);
                    while !content.is_empty() {
                        events.push(content.parse()?);
                        if content.peek(Token![,]) {
                            content.parse::<Token![,]>()?;
                        }
                    }
                }
                _ => return Err(syn::Error::new(key.span(), "unknown region machine field")),
            }
            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
        }
        let name = name.ok_or_else(|| input.error("missing machine name"))?;
        let result = Self {
            name,
            regions,
            events,
            snapshot,
        };
        result.validate()?;
        Ok(result)
    }
}
impl Parse for Event {
    fn parse(input: ParseStream<'_>) -> Result<Self> {
        let name = input.parse()?;
        let content;
        braced!(content in input);
        let mut payload = None;
        let mut routes = Vec::new();
        let mut fields = HashSet::new();
        while !content.is_empty() {
            let key: Ident = content.parse()?;
            content.parse::<Token![:]>()?;
            if !fields.insert(key.to_string()) {
                return Err(syn::Error::new(key.span(), "duplicate region event field"));
            }
            match key.to_string().as_str() {
                "payload" => payload = Some(content.parse()?),
                "routes" => routes = parse_named(&content)?,
                _ => return Err(syn::Error::new(key.span(), "unknown region event field")),
            }
            if !content.is_empty() {
                content.parse::<Token![,]>()?;
            }
        }
        Ok(Self {
            name,
            payload,
            routes,
        })
    }
}

impl Regions {
    fn validate(&self) -> Result<()> {
        if self.regions.len() < 2 {
            return Err(syn::Error::new(
                self.name.span(),
                "orthogonal machines need at least two regions",
            ));
        }
        let mut names = HashSet::new();
        let mut accessors: HashSet<String> = [
            "new",
            "inner",
            "handle",
            "start",
            "tick",
            "drain",
            "poll_activities",
            "is_finished",
            "is_poisoned",
            "take_join",
            "current_state",
            "schema",
            "into_regions",
            "into_snapshot",
            "try_into_snapshot",
            "from_snapshot",
            "validate_snapshot",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        for (name, _) in &self.regions {
            if name.to_string().starts_with("__sm_") {
                return Err(syn::Error::new(
                    name.span(),
                    "reserved internal region name",
                ));
            }
            if !names.insert(name.to_string()) {
                return Err(syn::Error::new(name.span(), "duplicate region"));
            }
            if !accessors.insert(name.to_string()) || !accessors.insert(format!("{name}_mut")) {
                return Err(syn::Error::new(
                    name.span(),
                    "region name conflicts with generated API",
                ));
            }
        }
        let mut events = HashSet::new();
        for event in &self.events {
            if !events.insert(to_pascal_case(&event.name.to_string())) {
                return Err(syn::Error::new(event.name.span(), "duplicate region event"));
            }
            if event.routes.is_empty() {
                return Err(syn::Error::new(
                    event.name.span(),
                    "region event needs a route",
                ));
            }
            let mut routed = HashSet::new();
            for (region, _) in &event.routes {
                if !names.contains(&region.to_string()) || !routed.insert(region.to_string()) {
                    return Err(syn::Error::new(
                        region.span(),
                        "unknown or duplicate routed region",
                    ));
                }
            }
        }
        Ok(())
    }

    /// Right-associated binary composition supports any number of named regions.
    fn composition(&self, index: usize) -> TokenStream {
        let ty = &self.regions[index].1;
        let left = quote! { ::state_machines::runtime::Region<#ty> };
        if index + 1 == self.regions.len() {
            return left;
        }
        let right = self.composition(index + 1);
        quote! { ::state_machines::runtime::Parallel<#left, #right> }
    }
    fn construct(&self, index: usize) -> TokenStream {
        let name = &self.regions[index].0;
        let left = quote! { ::state_machines::runtime::Region::new(#name, capacity) };
        if index + 1 == self.regions.len() {
            return left;
        }
        let right = self.construct(index + 1);
        quote! { ::state_machines::runtime::Parallel::new(#left, #right) }
    }
    fn route(&self, event: &Event, index: usize) -> Option<TokenStream> {
        let left = event
            .routes
            .iter()
            .find(|(name, _)| name == &self.regions[index].0)
            .map(|(_, expr)| quote! { #expr });
        if index + 1 == self.regions.len() {
            return left;
        }
        let right = self.route(event, index + 1);
        match (left, right) {
            (Some(left), Some(right)) => Some(
                quote! { ::state_machines::runtime::ParallelEvent::Both { left: #left, right: #right } },
            ),
            (Some(left), None) => {
                Some(quote! { ::state_machines::runtime::ParallelEvent::Left(#left) })
            }
            (None, Some(right)) => {
                Some(quote! { ::state_machines::runtime::ParallelEvent::Right(#right) })
            }
            (None, None) => None,
        }
    }
    fn access(&self, index: usize, mutable: bool) -> TokenStream {
        let right = if mutable {
            quote! { right_mut() }
        } else {
            quote! { right() }
        };
        let left = if mutable {
            quote! { left_mut() }
        } else {
            quote! { left() }
        };
        let mut access = quote! { self.inner };
        for _ in 0..index {
            access = quote! { #access.#right };
        }
        if index + 1 < self.regions.len() {
            access = quote! { #access.#left };
        }
        access
    }
    fn configuration(&self, value: TokenStream) -> TokenStream {
        let config = format_ident!("{}State", self.name);
        let fields = self.regions.iter().enumerate().map(|(index, (name, _))| {
            let mut value = value.clone();
            for _ in 0..index {
                value = quote! { #value.1 };
            }
            if index + 1 < self.regions.len() {
                value = quote! { #value.0 };
            }
            quote! { #name: #value }
        });
        quote! { #config { #(#fields,)* } }
    }

    fn split_regions(&self) -> TokenStream {
        let mut value = quote! { self.inner };
        let mut bindings = TokenStream::new();
        for (index, (name, _)) in self.regions.iter().enumerate() {
            if index + 1 == self.regions.len() {
                bindings.extend(quote! { let #name = #value; });
            } else {
                let rest = format_ident!("__sm_regions_{}", index);
                bindings.extend(quote! { let (#name, #rest) = #value.into_regions(); });
                value = quote! { #rest };
            }
        }
        bindings
    }

    fn snapshots(&self) -> TokenStream {
        if !self.snapshot {
            return TokenStream::new();
        }
        let name = &self.name;
        let snapshot = format_ident!("{}Snapshot", name);
        let machine = name.to_string();
        let fields = self.regions.iter().map(|(name, ty)| {
            quote! {
                pub #name: <#ty as ::state_machines::runtime::SnapshotMachine>::Snapshot
            }
        });
        let names: Vec<_> = self.regions.iter().map(|(name, _)| name).collect();
        let captures = names.iter().map(|name| {
            quote! {
                #name: ::state_machines::runtime::SnapshotMachine::into_snapshot(#name)
            }
        });
        let validates = self.regions.iter().map(|(name, ty)| quote! {
            <#ty as ::state_machines::runtime::SnapshotMachine>::validate_snapshot(&snapshot.#name)?;
        });
        let restores = self.regions.iter().map(|(name, ty)| quote! {
            <#ty as ::state_machines::runtime::SnapshotMachine>::from_validated_snapshot(snapshot.#name, capacity)
        });
        quote! {
            ::state_machines::__sm_if_serde! {
                #[derive(Debug, ::state_machines::__private::serde::Serialize, ::state_machines::__private::serde::Deserialize)]
                #[serde(crate = "::state_machines::__private::serde", deny_unknown_fields)]
                pub struct #snapshot {
                    pub version: u32,
                    pub machine: ::state_machines::__private::String,
                    #(#fields,)*
                }
                impl #name {
                    pub fn into_snapshot(self) -> #snapshot {
                        ::state_machines::runtime::SnapshotMachine::into_snapshot(self)
                    }
                    pub fn try_into_snapshot(self) -> Result<#snapshot, Self> {
                        ::state_machines::runtime::SnapshotMachine::try_into_snapshot(self)
                    }
                    pub fn validate_snapshot(snapshot: &#snapshot) -> Result<(), ::state_machines::SnapshotError> {
                        <Self as ::state_machines::runtime::SnapshotMachine>::validate_snapshot(snapshot)
                    }
                    pub fn from_snapshot(snapshot: #snapshot, capacity: usize) -> Result<Self, (#snapshot, ::state_machines::SnapshotError)> {
                        <Self as ::state_machines::runtime::SnapshotMachine>::from_snapshot(snapshot, capacity)
                    }
                }
                impl ::state_machines::runtime::SnapshotMachine for #name {
                    type Snapshot = #snapshot;
                    fn validate_snapshot(snapshot: &Self::Snapshot) -> Result<(), ::state_machines::SnapshotError> {
                        ::state_machines::SnapshotError::validate_header(snapshot.version, &snapshot.machine, #machine)?;
                        #(#validates)*
                        Ok(())
                    }
                    fn into_snapshot(self) -> Self::Snapshot {
                        assert!(!self.is_poisoned(), "cannot snapshot poisoned regions");
                        let (#(#names,)*) = self.into_regions();
                        #snapshot { version: 1, machine: #machine.into(), #(#captures,)* }
                    }
                    fn from_validated_snapshot(snapshot: Self::Snapshot, capacity: usize) -> Self {
                        Self::new(#(#restores,)* capacity)
                    }
                }
            }
        }
    }

    pub fn expand(&self) -> TokenStream {
        let name = &self.name;
        let event_name = format_ident!("{}Event", name);
        let config = format_ident!("{}State", name);
        let error = format_ident!("{}Error", name);
        let inner = self.composition(0);
        let construct = self.construct(0);
        let split = self.split_regions();
        let snapshots = self.snapshots();
        let region_types = self.regions.iter().map(|(_, ty)| ty);
        let region_values = self
            .regions
            .iter()
            .map(|(name, _)| quote! { #name.into_machine() });
        let parameters = self.regions.iter().map(|(name, ty)| quote! { #name: #ty });
        let config_fields = self.regions.iter().map(
            |(name, ty)| quote! { pub #name: <#ty as ::state_machines::runtime::Machine>::State },
        );
        let variants = self.events.iter().map(|event| {
            let variant = format_ident!("{}", to_pascal_case(&event.name.to_string()));
            let payload = event.payload.as_ref().map(|ty| quote! { (#ty) });
            quote! { #variant #payload }
        });
        let handlers = self.events.iter().map(|event| {
            let variant = format_ident!("{}", to_pascal_case(&event.name.to_string()));
            let pattern = event.payload.as_ref().map(|_| quote! { (payload) });
            let route = self.route(event, 0).expect("validated nonempty routes");
            quote! { #event_name::#variant #pattern => self.inner.handle(#route).await }
        });
        let accessors = self.regions.iter().enumerate().map(|(index, (name, ty))| {
            let mutable = format_ident!("{}_mut", name);
            let get = self.access(index, false);
            let get_mut = self.access(index, true);
            quote! {
                pub fn #name(&self) -> &::state_machines::runtime::Region<#ty> { #get }
                pub fn #mutable(&mut self) -> &mut ::state_machines::runtime::Region<#ty> { #get_mut }
            }
        });
        let scope_arms = self.regions.iter().enumerate().map(|(index, (region, _))| {
            let region = region.to_string();
            let get = self.access(index, false);
            quote! {
                #region => Some(0),
                _ if let Some(nested) = scope.strip_prefix(concat!(#region, "/")) =>
                    ::state_machines::runtime::Machine::scope_epoch(#get, nested),
            }
        });
        let current = self.configuration(quote! { state });
        let join = self.configuration(quote! { state });
        let name_str = name.to_string();
        let region_metadata = self.regions.iter().map(|(name, ty)| {
            let name = name.to_string();
            let ty = quote! { #ty }.to_string();
            quote! { ::state_machines::RegionSchema { name: #name.into(), machine: #ty.into() } }
        });
        let event_metadata = self.events.iter().map(|event| {
            let name = event.name.to_string();
            let payload = event.payload.as_ref().map_or(quote! { None }, |ty| {
                let value = quote! { #ty }.to_string();
                quote! { Some(#value.into()) }
            });
            let routes = event.routes.iter().map(|(region, _)| region.to_string());
            quote! { ::state_machines::RegionEventSchema { name: #name.into(), payload: #payload,
            regions: ::state_machines::__private::vec![#(#routes.into()),*] } }
        });
        quote! {
            ::state_machines::__sm_require_runtime!();
            ::state_machines::__sm_if_runtime! {
                #[derive(Debug)]
                pub enum #event_name { #(#variants,)* }
                #[derive(Debug, Copy, Clone, PartialEq, Eq)]
                pub struct #config { #(#config_fields,)* }
                pub type #error = <#inner as ::state_machines::runtime::Machine>::Error;
                pub struct #name { inner: #inner }
                impl #name {
                    pub fn new(#(#parameters,)* capacity: usize) -> Self { Self { inner: #construct } }
                    pub fn into_regions(self) -> (#(#region_types,)*) {
                        #split
                        (#(#region_values,)*)
                    }
                    #(#accessors)*
                    pub fn current_state(&self) -> #config {
                        let state = self.inner.current_state(); #current
                    }
                    pub fn is_finished(&self) -> bool { self.inner.is_finished() }
                    pub fn is_poisoned(&self) -> bool { self.inner.is_poisoned() }
                    pub fn take_join(&mut self) -> Option<#config> {
                        self.inner.take_join().map(|state| #join)
                    }
                    pub async fn handle(&mut self, event: #event_name) -> Result<(), #error> {
                        match event { #(#handlers,)* }
                    }
                    pub fn start(&mut self, clock: &impl ::state_machines::runtime::Clock) -> Result<(), #error> {
                        ::state_machines::runtime::Machine::start_regions(&mut self.inner, clock)
                    }
                    pub fn tick(&mut self, clock: &impl ::state_machines::runtime::Clock) -> Result<(), #error> {
                        ::state_machines::runtime::Machine::tick_regions(&mut self.inner, clock)
                    }
                    pub fn poll_activities(&mut self, cx: &mut ::core::task::Context<'_>) -> usize {
                        ::state_machines::runtime::Machine::poll_regions(&mut self.inner, cx)
                    }
                    pub async fn drain(&mut self, max_steps: usize) -> Result<usize, #error> {
                        ::state_machines::runtime::Machine::drive_regions(&mut self.inner, max_steps).await
                    }
                    ::state_machines::__sm_if_inspect! {
                        pub fn schema() -> ::state_machines::MachineSchema {
                            ::state_machines::MachineSchema { name: #name_str.into(),
                                regions: ::state_machines::__private::vec![#(#region_metadata,)*],
                                region_events: ::state_machines::__private::vec![#(#event_metadata,)*],
                                ..::core::default::Default::default()
                            }
                        }
                    }
                }
                impl ::state_machines::runtime::Machine for #name {
                    type Event = #event_name;
                    type Error = #error;
                    type State = #config;
                    fn state(&self) -> Self::State { self.current_state() }
                    fn epoch(&self) -> u64 { ::state_machines::runtime::Machine::epoch(&self.inner) }
                    fn scope_epoch(&self, scope: &str) -> Option<u64> {
                        if self.is_poisoned() { return None; }
                        match scope { #(#scope_arms)* _ => None }
                    }
                    fn is_finished(&self) -> bool { self.is_finished() }
                    fn is_poisoned(&self) -> bool { self.is_poisoned() }
                    async fn dispatch(&mut self, event: Self::Event) -> Result<(), Self::Error> { self.handle(event).await }
                    fn start_regions(&mut self, clock: &impl ::state_machines::runtime::Clock) -> Result<(), Self::Error> { self.start(clock) }
                    fn tick_regions(&mut self, clock: &impl ::state_machines::runtime::Clock) -> Result<(), Self::Error> { self.tick(clock) }
                    fn poll_regions(&mut self, cx: &mut ::core::task::Context<'_>) -> usize { self.poll_activities(cx) }
                    async fn drive_regions(&mut self, max_steps: usize) -> Result<usize, Self::Error> { self.drain(max_steps).await }
                }
                #snapshots
            }
        }
    }
}

#[cfg(test)]
mod tests;

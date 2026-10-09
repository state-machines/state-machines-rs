# state-machines

> **A learning-focused Rust port of Ruby's state_machines gem**

[![Crates.io](https://img.shields.io/crates/v/state-machines.svg)](https://crates.io/crates/state-machines)
[![Documentation](https://docs.rs/state-machines/badge.svg)](https://docs.rs/state-machines)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)
[![GitHub](https://img.shields.io/badge/github-state--machines/state--machines--rs-blue)](https://github.com/state-machines/state-machines-rs)

## About This Project

This is a Rust port of the popular [state_machines](https://github.com/state-machines/state_machines) Ruby gem, created as a **learning platform for Rubyists transitioning to Rust**.

While learning Rust, I chose to port something familiar and widely used—so I could compare implementations side-by-side and understand Rust's patterns through a lens I already knew. This library is intentionally **over-commented**, not because the code is disorganized, but because it's designed to be a **teaching tool**. The goal is elegant, idiomatic Rust code that Rubyists can learn from without the usual compile-pray-repeat cycle.

### Philosophy

- **Learning Ground First**: Extensive inline comments explain Rust concepts, ownership, trait bounds, and macro magic
- **Ruby Parallels**: Familiar DSL syntax and callbacks make the transition smoother
- **Production Ready**: Despite the educational focus, this is a fully functional state machine library with:
  - **Typestate pattern** for compile-time state safety
  - **Zero-cost abstractions** using PhantomData
  - Guards and unless conditions
  - Before/after event callbacks
  - Sync and async support
  - `no_std` compatibility (for embedded systems)
  - Payload support for event data
  - Move semantics preventing invalid state transitions

### For the Rust Community

**You're welcome to open PRs** to fix fundamentally wrong Rust concepts—but please **don't remove comments just because "we know it"**. This codebase serves beginners. If something can be explained better, improve the comment. If a pattern is unidiomatic, fix it *and document why*.

---

## Features

**Typestate Pattern** – Compile-time state safety using Rust's type system with zero runtime overhead

**Guards & Unless** – Conditional transitions at event and transition levels, plus non-consuming `can_<event>()` predicates

**Callbacks** – `before`/`after` hooks at event and transition level

**Global Filtered Callbacks** – Machine-wide `callbacks:` block with `from`/`to`/`on` filters; filtering on `to:`/`from:` doubles as state enter/exit hooks

**Around Callbacks** – Wrap transitions with Before/AfterSuccess stages for transaction-like semantics

**Async Support** – First-class `async`/`await` for guards and callbacks

**Event Payloads** – Pass data through transitions with type-safe payloads

**No-std Compatible** – Works on embedded targets (ESP32, bare metal)

**Type-safe** – Invalid transitions become compile errors, not runtime errors

**Hierarchical States** – Superstates with polymorphic transitions via SubstateOf trait, and superstate data that lives for the whole region

**Dynamic Dispatch** – Runtime event dispatch for event-driven systems (opt-in via feature flag or explicit config)

**State Data Accessors** – Access and mutate per-state data in dynamic mode

**Introspection** – `schema()` metadata with JSON and Mermaid rendering (via the `inspect` feature, implied by `std`)

**Graph Validation** – `schema().validate()` reports invalid references and ambiguous
transitions as errors, and unreachable states/dead ends as warnings. The CLI's
`validate` command runs these checks too. Reachability is structural, not a
prediction of user guards.

**Failure Hooks** – Event/transition `on_error: [cleanup]` and global
`callbacks: { error_transition [{ name: cleanup }] }` run once on the recovered
source machine when a guard, around callback, or fallible callback rejects.
Hooks take `&GuardError` (or `&EventError<E>` with `error: E`) and return `()`;
async machines await them. They do not run for panics, cancellation, or invalid
dynamic events, and do not undo external side effects.

**Internal Transitions** – `transition: { from: [Active, Idle], internal: true }`
handles an event without changing state or resetting state data. Omit `to`;
guards and event callbacks still run. An ordinary `from: Active, to: Active`
transition is external and resets leaf data.

**Hierarchical Lifecycle** – Declare
`lifecycle: { Flight { enter: [open_region], exit: [close_region] } }`.
After guards and before callbacks, exit hooks run inner-to-outer; after the
state/data change, enter hooks run outer-to-inner before after callbacks.
Common ancestors stay active; external self-transitions re-enter the leaf,
internal transitions run neither. Hooks take no payload, support async and
fallible returns like event callbacks, and participate in failure recovery.
`new()` remains an infallible constructor and does not invoke entry hooks.

**Final States and Completion** – `final_states: [Done]` declares terminal leaves
and rejects directly declared outgoing transitions. Inherited parent exits remain
legal. `is_finished()` identifies a root final;
`completion_events()` reports `CompletionEvent::Machine` or
`CompletionEvent::Superstate("Parent")` for a nested final's immediate parent.
Dynamic `take_completion_events()` drains notifications from successful
`handle()`/`stabilize()` calls only; runtime-driven dispatch (`Runner`,
`Parallel`, regions) does not queue them. A parent's `lifecycle: { Parent { complete: [notify] } }`
hook runs after the ordinary transition callbacks. Completion does not
implicitly mark every ancestor complete. Use parent completion triggers for
automatic progression.

**Guarded Branching** – Opt in with event `branching: true`; candidates from the
same source are tried in declaration order. Each needs transition-level
`guards`/`unless`, except an optional last `fallback: true` candidate.
Event guards run once, then candidate guards run once; only the selected
transition runs callbacks. No match returns a `branch_selection` guard error.
For multiple candidates, typestate returns
`<Machine><Source><Event>Outcome` with one typed variant per destination;
dynamic dispatch selects the same outcome without duplicate available events.
Branch selection precedes around/before callbacks.

**Snapshot/Restore** – Enable the optional `serde` feature and declare
`dynamic: true, snapshot: true`. `into_snapshot()` consumes a dynamic machine
into its generated `<Machine>Snapshot<C>`: version, machine identity, active
leaf, context, and leaf/superstate data. No `Clone` bound is needed.
`Dynamic<Machine>::from_snapshot(snapshot)` validates version, identity, state,
and inactive-data consistency, returning the intact snapshot on error.
Context/data must support Serde. Restore invokes no callbacks and emits no
completion notifications; pending notifications are not persisted.
Active data may be `None`, matching the existing constructors' lazy data
initialization. Version 1 describes the snapshot format, not automatic
application-schema migration. The feature works with `no_std` + `alloc`.

**History States** – `transition: { from: Paused, to: Running, history: deep }`
resumes a superstate's last active leaf; `history: shallow` resumes its last
direct child, entering that child's initial leaf if it is composite. Unvisited
history uses the region's initial child. History is recorded only on successful
exits, survives typestate/dynamic conversions and snapshots, and is validated
on restore. Multi-destination history returns the same generated typed outcome
enums as branching. Guards, lifecycle hooks, and async dispatch still apply.
History remembers control state only: exited state data is cleared and
re-entered data is initialized normally. Only regions used as history targets
gain optional, allocation-free history storage.

---

## Quick Start

Requires Rust 1.99 or newer. Development and CI use Rust 1.99.0
(`mise install` sets up the pinned toolchain).

Add to your `Cargo.toml`:

```toml
[dependencies]
state-machines = "0.21"
```

### Basic Example

```rust
use state_machines::state_machine;

// Define your state machine
state_machine! {
    name: TrafficLight,

    initial: Red,
    states: [Red, Yellow, Green],
    events {
        next {
            transition: { from: Red, to: Green }
            transition: { from: Green, to: Yellow }
            transition: { from: Yellow, to: Red }
        }
    }
}

fn main() {
    // Typestate pattern: each transition returns a new typed machine
    let light = TrafficLight::new(());
    // Type is TrafficLight<Red>

    let light = light.next().unwrap();
    // Type is TrafficLight<Green>

    let light = light.next().unwrap();
    // Type is TrafficLight<Yellow>
}
```

### With Guards and Callbacks

```rust
use state_machines::{state_machine, core::GuardError};
use std::sync::atomic::{AtomicBool, Ordering};

static DOOR_OBSTRUCTED: AtomicBool = AtomicBool::new(false);

state_machine! {
    name: Door,

    initial: Closed,
    states: [Closed, Open],
    events {
        open {
            guards: [path_clear],
            before: [check_safety],
            after: [log_opened],
            transition: { from: Closed, to: Open }
        }
        close {
            transition: { from: Open, to: Closed }
        }
    }
}

impl<C, S> Door<C, S> {
    fn path_clear(&self, _ctx: &C) -> bool {
        !DOOR_OBSTRUCTED.load(Ordering::Relaxed)
    }

    fn check_safety(&self) {
        println!("Checking if path is clear...");
    }

    fn log_opened(&self) {
        println!("Door opened at {:?}", std::time::SystemTime::now());
    }
}

fn main() {
    // Successful transition
    let door = Door::new(());
    let door = door.open().unwrap();
    let door = door.close().unwrap();

    // Failed guard check
    DOOR_OBSTRUCTED.store(true, Ordering::Relaxed);
    let err = door.open().expect_err("should fail when obstructed");
    let (_door, guard_err) = err;
    assert_eq!(guard_err.guard, "path_clear");

    // Inspect the error kind
    use state_machines::core::TransitionErrorKind;
    match guard_err.kind {
        TransitionErrorKind::GuardFailed { guard } => {
            println!("Guard '{}' failed", guard);
        }
        _ => unreachable!(),
    }
}
```

### Concrete Context for Embedded Systems

For embedded systems or applications where the context type is known at compile time, you can specify a **concrete context type** in the macro. This allows guards and callbacks to directly access context fields without generic trait bounds.

**Generic Context (Default):**
```rust,ignore
state_machine! {
    name: Door,
    // No context specified - machine is generic over C
}

impl<C, S> Door<C, S> {
    fn guard(&self, _ctx: &C) -> bool {
        // C is generic - can't access its fields
        false
    }
}
```

**Concrete Context (Embedded-Friendly):**
```rust
use state_machines::state_machine;

#[derive(Debug, Default)]
struct HardwareSensors {
    temperature_c: i16,
    pressure_kpa: u32,
}

state_machine! {
    name: Door,
    context: HardwareSensors,  // ← Concrete context type
    initial: Closed,
    states: [Closed, Open],
    events {
        open {
            guards: [safe_conditions],
            transition: { from: Closed, to: Open }
        }
        close {
            transition: { from: Open, to: Closed }
        }
    }
}

impl<S> Door<S> {
    fn safe_conditions(&self, ctx: &HardwareSensors) -> bool {
        // Direct field access!
        ctx.temperature_c >= -40
            && ctx.temperature_c <= 85
            && ctx.pressure_kpa >= 95
            && ctx.pressure_kpa <= 105
    }
}

fn main() {
    let sensors = HardwareSensors {
        temperature_c: 22,
        pressure_kpa: 101,
    };

    let door = Door::new(sensors);
    let door = door.open().unwrap();
    let _door = door.close().unwrap();
}
```

**Key Differences:**

| Aspect | Generic Context | Concrete Context |
|--------|----------------|------------------|
| **Struct signature** | `Machine<C, S>` | `Machine<S>` |
| **Impl blocks** | `impl<C, S>` | `impl<S>` |
| **Guard signature** | `fn(&self, &C)` | `fn(&self, &HardwareType)` |
| **Field access** | Not possible | Direct access |
| **Flexibility** | Works with any context | Fixed to one type |
| **Use case** | Libraries, flexibility | Embedded, hardware |

**When to Use:**
- **Embedded systems** – Hardware types known at compile time
- **no_std environments** – Direct hardware register access
- **Fixed architectures** – Single deployment target
- **Performance critical** – Compiler can optimize better

**When to Avoid:**
- **Libraries** – Users need context flexibility
- **Multiple deployments** – Different hardware configs
- **Generic code** – Need to work with various types

See `examples/guards_and_validation` for a complete example using concrete context for spacecraft telemetry.

### Async Support

Async state machines are behind the `async` feature:

```toml
[dependencies]
state-machines = { version = "0.21", features = ["async"] }
```

The typestate pattern works seamlessly with async Rust:

```rust,ignore
use state_machines::state_machine;

state_machine! {
    name: HttpRequest,

    initial: Idle,
    async: true,
    states: [Idle, Pending, Success, Failed],
    events {
        send {
            guards: [has_network],
            transition: { from: Idle, to: Pending }
        }
        succeed {
            transition: { from: Pending, to: Success }
        }
        fail {
            transition: { from: Pending, to: Failed }
        }
    }
}

impl<C, S> HttpRequest<C, S> {
    async fn has_network(&self, _ctx: &C) -> bool {
        // Async guard checks network availability
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        true
    }
}

// If callbacks can fail, declare an error type for the machine.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HttpError {
    Timeout,
}

state_machine! {
    name: AuthRecovery,
    async: true,
    error: HttpError,
    initial: RefreshToken,
    states: [RefreshToken, Done],
    events {
        refresh {
            before: [refresh_token],
            transition: { from: RefreshToken, to: Done }
        }
    }
}

impl<C, S> AuthRecovery<C, S> {
    async fn refresh_token(&self) -> Result<(), HttpError> {
        Err(HttpError::Timeout)
    }
}

#[tokio::main]
async fn main() {
    // Type: HttpRequest<Idle>
    let request = HttpRequest::new(());

    // Type: HttpRequest<Pending>
    let request = request.send().await.unwrap();

    // Type: HttpRequest<Success>
    let request = request.succeed().await.unwrap();

    // `before`/`after` callback failures do not advance the state.
    match AuthRecovery::new(()).refresh().await {
        Err((_machine, state_machines::EventError::Callback(err))) => {
            assert_eq!(err.action, "refresh_token");
            assert_eq!(err.source, HttpError::Timeout);
        }
        _ => unreachable!(),
    }
}
```

### Event Payloads

```rust
use state_machines::state_machine;

#[derive(Clone, Debug)]
struct LoginCredentials {
    username: String,
    password: String,
}

state_machine! {
    name: AuthSession,
    initial: LoggedOut,
    states: [LoggedOut, LoggedIn, Locked],
    events {
        login {
            payload: LoginCredentials,
            guards: [valid_credentials],
            transition: { from: LoggedOut, to: LoggedIn }
        }
        logout {
            transition: { from: LoggedIn, to: LoggedOut }
        }
    }
}

impl<C, S> AuthSession<C, S> {
    fn valid_credentials(&self, _ctx: &C, creds: &LoginCredentials) -> bool {
        // Guard receives context and payload reference
        creds.username == "admin" && creds.password == "secret"
    }
}

fn main() {
    let session = AuthSession::new(());
    // Type is AuthSession<(), LoggedOut>

    let good_creds = LoginCredentials {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let session = session.login(good_creds).unwrap();
    // Type is AuthSession<LoggedIn>
}
```

### Hierarchical States (Superstates)

Group related states into superstates for polymorphic transitions and cleaner state organization:

```rust
use state_machines::state_machine;

#[derive(Default, Debug, Clone)]
struct PrepData {
    checklist_complete: bool,
}

#[derive(Default, Debug, Clone)]
struct LaunchData {
    engines_ignited: bool,
}

state_machine! {
    name: LaunchSequence,

    initial: Standby,
    states: [
        Standby,
        superstate Flight {
            state LaunchPrep(PrepData),
            state Launching(LaunchData),
        },
        InOrbit,
    ],
    events {
        enter_flight {
            transition: { from: Standby, to: Flight }
        }
        ignite {
            transition: { from: Standby, to: LaunchPrep }
        }
        cycle_engines {
            transition: { from: LaunchPrep, to: Launching }
        }
        ascend {
            transition: { from: Flight, to: InOrbit }
        }
        abort {
            transition: { from: Flight, to: Standby }
        }
    }
}

fn main() {
    // Start in Standby
    let sequence = LaunchSequence::new(());

    // Transition to Flight superstate resolves to initial child (LaunchPrep)
    let sequence = sequence.enter_flight().unwrap();

    // This transition initialized the destination's data.
    let prep_data = sequence.launch_prep_data().expect("initialized on entry");
    println!("Checklist complete: {}", prep_data.checklist_complete);

    // Move to Launching within Flight superstate
    let sequence = sequence.cycle_engines().unwrap();

    // abort() is defined on Flight, but works from ANY substate
    let sequence = sequence.abort().unwrap();
    // Type: LaunchSequence<C, Standby>

    // Go directly to LaunchPrep (bypassing superstate entry)
    let sequence = sequence.ignite().unwrap();
    // Type: LaunchSequence<C, LaunchPrep>

    // abort() STILL works - polymorphic transition!
    let _sequence = sequence.abort().unwrap();
}
```

**Key Features:**

- **Polymorphic Transitions**: Define transitions `from: Flight` that work from ANY substate (LaunchPrep, Launching)
- **Automatic Resolution**: `to: Flight` transitions resolve to the superstate's initial child state
- **State Data Storage**: Each state with data gets optional-reference accessors like `launch_prep_data()` and `launching_data()`; constructors/restore may leave data absent
- **SubstateOf Trait**: Generated trait implementations enable compile-time polymorphism
- **Storage Lifecycle**: State data is automatically initialized on entry, cleared on exit

**Under the Hood:**

The macro generates:

```rust,ignore
// Marker trait for polymorphism
impl SubstateOf<Flight> for LaunchPrep {}
impl SubstateOf<Flight> for Launching {}

// Polymorphic transition implementation
impl<C, S: SubstateOf<Flight>> LaunchSequence<C, S> {
    pub fn abort(self) -> Result<LaunchSequence<C, Standby>, ...> {
        // Works from ANY state where S implements SubstateOf<Flight>
    }
}

// State-specific data accessors (no Option wrapper!)
impl<C> LaunchSequence<C, LaunchPrep> {
    pub fn launch_prep_data(&self) -> &PrepData { ... }
    pub fn launch_prep_data_mut(&mut self) -> &mut PrepData { ... }
}
```

**Ruby Comparison:**

Ruby's `state_machines` doesn't have formal superstate support in this way. The closest equivalent would be using state predicates:

```ruby
# Ruby approach
def in_flight?
  [:launch_prep, :launching].include?(state)
end

# Rust: Compile-time polymorphism via trait bounds
impl<C, S: SubstateOf<Flight>> LaunchSequence<C, S> {
  pub fn abort(self) -> ... { }
}
```

Rust's typestate pattern makes this compile-time safe with zero runtime overhead.

---

### Around Callbacks

Around callbacks wrap transitions with **transaction-like semantics**, providing Before and AfterSuccess hooks that bracket the entire transition execution:

```rust
use state_machines::{state_machine, core::{AroundStage, AroundOutcome}};
use std::sync::atomic::{AtomicUsize, Ordering};

static CALL_COUNT: AtomicUsize = AtomicUsize::new(0);

state_machine! {
    name: Transaction,
    initial: Idle,
    states: [Idle, Processing, Complete],
    events {
        begin {
            around: [transaction_wrapper],
            transition: { from: Idle, to: Processing }
        }
        succeed {
            transition: { from: Processing, to: Complete }
        }
    }
}

impl<C, S> Transaction<C, S> {
    fn transaction_wrapper(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        match stage {
            AroundStage::Before => {
                println!("Starting transaction...");
                CALL_COUNT.fetch_add(1, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess => {
                println!("Transaction committed!");
                CALL_COUNT.fetch_add(10, Ordering::SeqCst);
                AroundOutcome::Proceed
            }
        }
    }
}

fn main() {
    let transaction = Transaction::new(());
    let transaction = transaction.begin().unwrap();

    // CALL_COUNT is now 11 (Before: +1, AfterSuccess: +10)
    assert_eq!(CALL_COUNT.load(Ordering::SeqCst), 11);
}
```

**Execution Order:**

1. **Around Before** – Runs first, can abort the entire transition
2. **Guards** – Event/transition guards evaluated
3. **Before callbacks** – Event-level before hooks
4. **State transition** – Actual state change occurs
5. **After callbacks** – Event-level after hooks
6. **Around AfterSuccess** – Runs last, guaranteed to execute after successful transition

**Aborting Transitions:**

Around callbacks at the Before stage can abort transitions by returning `AroundOutcome::Abort`:

```rust
use state_machines::{
    state_machine,
    core::{AroundStage, AroundOutcome, TransitionError},
};

state_machine! {
    name: Guarded,
    initial: Start,
    states: [Start, End],
    events {
        advance {
            around: [abort_guard],
            transition: { from: Start, to: End }
        }
    }
}

impl<C, S> Guarded<C, S> {
    fn abort_guard(&self, stage: AroundStage) -> AroundOutcome<Start> {
        match stage {
            AroundStage::Before => {
                // Abort at Before stage
                AroundOutcome::Abort(TransitionError::guard_failed(
                    Start,
                    "advance",
                    "abort_guard",
                ))
            }
            AroundStage::AfterSuccess => {
                // Won't be called when Before aborts
                AroundOutcome::Proceed
            }
        }
    }
}

fn main() {
    let machine = Guarded::new(());
    let result = machine.advance();

    assert!(result.is_err());
    let (_machine, err) = result.unwrap_err();
    assert_eq!(err.guard, "abort_guard");
}
```

**Distinguishing Error Types:**

Around callbacks preserve the full `TransitionErrorKind`, allowing you to distinguish between guard failures and action failures:

```rust
use state_machines::{
    state_machine,
    core::{AroundStage, AroundOutcome, TransitionError, TransitionErrorKind},
};

state_machine! {
    name: Workflow,
    initial: Pending,
    states: [Pending, Validated, Complete],
    events {
        validate {
            around: [validation_wrapper],
            transition: { from: Pending, to: Validated }
        }
    }
}

impl<C, S> Workflow<C, S> {
    fn validation_wrapper(&self, stage: AroundStage) -> AroundOutcome<Pending> {
        match stage {
            AroundStage::Before => {
                // Abort with ActionFailed (not GuardFailed)
                AroundOutcome::Abort(TransitionError {
                    from: Pending,
                    event: "validate",
                    kind: TransitionErrorKind::ActionFailed {
                        action: "validation_wrapper",
                    },
                })
            }
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }
}

fn main() {
    let workflow = Workflow::new(());
    let result = workflow.validate();

    if let Err((_workflow, err)) = result {
        // Inspect the error kind to distinguish failure types
        match err.kind {
            TransitionErrorKind::GuardFailed { guard } => {
                println!("Guard '{}' prevented transition", guard);
            }
            TransitionErrorKind::ActionFailed { action } => {
                println!("Action '{}' aborted transition", action);
            }
            TransitionErrorKind::InvalidTransition => {
                println!("Invalid state transition");
            }
        }
    }
}
```

**Use Cases:**

- **Database transactions** – Begin/commit semantics
- **Resource locking** – Acquire before, release after
- **Logging/tracing** – Instrument transitions
- **Performance monitoring** – Measure transition duration
- **Validation** – Pre/post-condition checks
- **Cleanup** – Ensure resources are released after transition

**Multiple Around Callbacks:**

You can specify multiple around callbacks that all execute in order:

```rust,ignore
state_machine! {
    name: Multi,
    initial: X,
    states: [X, Y],
    events {
        go {
            around: [logging_wrapper, metrics_wrapper, transaction_wrapper],
            transition: { from: X, to: Y }
        }
    }
}
```

All Before stages run in order, then the transition, then all AfterSuccess stages.

**Performance:**

Around callbacks achieve **zero-cost abstraction** when optimized:

| Configuration | Overhead | Notes |
|--------------|----------|-------|
| Single around callback | ~411 ps | Same as simple transition |
| Multiple around callbacks (3) | ~411 ps | Compiler optimizes away empty wrappers |
| Around + guards + callbacks | ~412 ps | All features combined, negligible overhead |

See `state-machines/benches/typestate_transitions.rs` for detailed benchmarks.

---

## Dynamic Dispatch Mode

While the typestate pattern provides excellent compile-time safety, sometimes you need **runtime flexibility** when events come from external sources (user input, network messages, event queues). Dynamic dispatch mode solves this by generating a runtime wrapper alongside your typestate machine.

### When to Use Dynamic Mode

**Use Typestate When:**
- ✅ Control flow is known at compile time
- ✅ Want maximum type safety
- ✅ Performance critical (zero overhead)
- ✅ Building DSLs or configuration pipelines

**Use Dynamic When:**
- ✅ Events from external sources (UI, network, queues)
- ✅ Runtime event routing/dispatch
- ✅ Need to store machines in collections
- ✅ Building event-driven systems or GUIs

**Use Both When:**
- ✅ Type-safe setup phase, then dynamic runtime
- ✅ Want compile-time safety where possible

### Enabling Dynamic Mode

Dynamic dispatch is **opt-in** to keep binaries small by default. Enable it via:

**Option 1: Explicit in macro (always generates dynamic code)**
```rust,ignore
state_machine! {
    name: TrafficLight,
    dynamic: true,  // ← Enable dynamic dispatch
    initial: Red,
    states: [Red, Yellow, Green],
    events { /* ... */ }
}
```

**Option 2: Cargo feature flag (conditional compilation)**
```toml
[dependencies]
state-machines = { version = "0.21", features = ["dynamic"] }
```

With the feature flag enabled, ALL state machines get dynamic dispatch without explicit `dynamic: true`.

### Basic Dynamic Dispatch

```rust,ignore
use state_machines::state_machine;

state_machine! {
    name: TrafficLight,
    dynamic: true,
    initial: Red,
    states: [Red, Yellow, Green],
    events {
        next {
            transition: { from: Red, to: Green }
            transition: { from: Green, to: Yellow }
            transition: { from: Yellow, to: Red }
        }
    }
}

fn main() {
    // Create dynamic machine
    let mut light = DynamicTrafficLight::new(());

    // Runtime event dispatch
    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Green);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Yellow);

    light.handle(TrafficLightEvent::Next).unwrap();
    assert_eq!(light.current_state(), TrafficLightState::Red);
}
```

### What Gets Generated

When `dynamic: true` is set, the macro generates:

1. **Event Enum** – Runtime representation of events
```rust
pub enum TrafficLightEvent {
    Next,
    // With payloads:
    // SetSpeed(u32),
}
```

2. **Dynamic Machine** – Runtime dispatch wrapper
```rust,ignore
pub struct DynamicTrafficLight<C> {
    // Internal state wrapper
}

pub enum TrafficLightState {
    Red,
    Yellow,
    Green,
}

impl<C> DynamicTrafficLight<C> {
    pub fn new(ctx: C) -> Self { /* Uses the declared initial state */ }
    pub fn new_init_state(ctx: C, state: TrafficLightState) -> Self { /* ... */ }
    pub fn handle(&mut self, event: TrafficLightEvent) -> Result<(), DynamicError> { /* ... */ }
    pub fn current_state(&self) -> TrafficLightState { /* ... */ }
    pub fn get_available_events(&self) -> Vec<TrafficLightEvent> { /* ... */ }
}
```

Use `new` for the state declared by `initial`, or `new_init_state` to select one explicitly:

```rust,ignore
let red = DynamicTrafficLight::new(());
let yellow =
    DynamicTrafficLight::new_init_state((), TrafficLightState::Yellow);
```

`get_available_events()` returns events valid from the current state whose
`guards` and `unless` checks pass:

```rust,ignore
let events = yellow.get_available_events();
assert_eq!(events[0].name(), "next");
```

Payload events are omitted because their guards cannot be evaluated without a
payload value. For async machines, use
`machine.get_available_events().await`.

3. **Conversion Methods** – Switch between modes
```rust,ignore
impl<C> TrafficLight<C, Red> {
    pub fn into_dynamic(self) -> DynamicTrafficLight<C> { /* ... */ }
}

impl<C> DynamicTrafficLight<C> {
    pub fn into_red(self) -> Result<TrafficLight<C, Red>, Self> { /* ... */ }
    pub fn into_yellow(self) -> Result<TrafficLight<C, Yellow>, Self> { /* ... */ }
    pub fn into_green(self) -> Result<TrafficLight<C, Green>, Self> { /* ... */ }
}
```

### Switching Between Modes

Convert from typestate to dynamic when you need runtime flexibility:

```rust,ignore
// Start with typestate for setup
let light = TrafficLight::new(());
// Type: TrafficLight<(), Red>

// Perform type-safe transitions
let light = light.next().unwrap();
// Type: TrafficLight<(), Green>

// Convert to dynamic for event loop
let mut dynamic_light = light.into_dynamic();

// Now handle runtime events
loop {
    let event = receive_event(); // From network, user input, etc
    match dynamic_light.handle(event) {
        Ok(()) => println!("Transitioned to {}", dynamic_light.current_state()),
        Err(e) => eprintln!("Transition failed: {:?}", e),
    }
}
```

Convert back to typestate when you know the current state:

```rust,ignore
let mut dynamic = DynamicTrafficLight::new(());
dynamic.handle(TrafficLightEvent::Next).unwrap();

// Extract typed machine if in Green state
if let Ok(typed) = dynamic.into_green() {
    // Type: TrafficLight<(), Green>
    // Now have compile-time guarantees again
    let _ = typed.next();
}
```

### Event-Driven Example

A common pattern is using dynamic mode with external event sources:

```rust
use state_machines::{state_machine, DynamicError};

state_machine! {
    name: Connection,
    dynamic: true,
    initial: Disconnected,
    states: [Disconnected, Connecting, Connected, Failed],
    events {
        connect {
            transition: { from: Disconnected, to: Connecting }
        }
        established {
            transition: { from: Connecting, to: Connected }
        }
        timeout {
            transition: { from: Connecting, to: Failed }
        }
        disconnect {
            transition: { from: [Connecting, Connected], to: Disconnected }
        }
    }
}

fn handle_network_events(conn: &mut DynamicConnection<()>) {
    // Receive events from network layer
    let events = vec![
        ConnectionEvent::Connect,
        ConnectionEvent::Established,
        ConnectionEvent::Disconnect,
    ];

    for event in events {
        match conn.handle(event) {
            Ok(()) => {
                println!("State: {}", conn.current_state());
            }
            Err(DynamicError::InvalidTransition { from, event }) => {
                eprintln!("Can't {} from {}", event, from);
            }
            Err(DynamicError::GuardFailed { guard, event }) => {
                eprintln!("Guard {} failed for {}", guard, event);
            }
            Err(DynamicError::ActionFailed { action, event }) => {
                eprintln!("Action {} failed for {}", action, event);
            }
            Err(DynamicError::CallbackFailed { action, event, source }) => {
                eprintln!("Callback {} failed for {}: {:?}", action, event, source);
            }
            Err(DynamicError::WrongState { expected, actual, operation }) => {
                eprintln!("Operation {} expected state {}, but in {}", operation, expected, actual);
            }
            Err(DynamicError::Poisoned { from, event }) => {
                eprintln!("Dispatch {} from {} was interrupted; replace the machine", event, from);
            }
            Err(DynamicError::StepLimit { limit }) => {
                eprintln!("Automatic transitions exceeded {} microsteps", limit);
            }
        }
    }
}

fn main() {
    let mut conn = DynamicConnection::new(());
    handle_network_events(&mut conn);
}
```

### Error Handling

Dynamic mode provides `DynamicError<E = ()>` with five variants:

```rust
pub enum DynamicError<E = ()> {
    InvalidTransition { from: &'static str, event: &'static str },
    GuardFailed { guard: &'static str, event: &'static str },
    ActionFailed { action: &'static str, event: &'static str },
    CallbackFailed { action: &'static str, event: &'static str, source: E },
    WrongState { expected: &'static str, actual: &'static str, operation: &'static str },
}
```

If your machine declares `error: AuthError`, dynamic dispatch uses `DynamicError<AuthError>`.
Guard and callback failures leave the wrapper in its source state.

```rust,ignore
let mut machine = DynamicTrafficLight::new(());

// Invalid transition
let result = machine.handle(TrafficLightEvent::Next); // Red → Green (valid)
assert!(result.is_ok());

// Machine is now in Green state, regardless of success/failure
assert_eq!(machine.current_state(), TrafficLightState::Green);
```

### State Data Accessors

Dynamic machines can access and mutate per-state data, enabling patterns like circuit breakers that need runtime counters and timestamps.

When states have associated data (e.g., `Open(OpenData)`), the macro generates three accessor types on the dynamic wrapper:

```rust,ignore
// Read-only access (returns None if not in this state)
pub fn open_data(&self) -> Option<&OpenData>

// Mutable access for updating counters/timestamps
pub fn open_data_mut(&mut self) -> Option<&mut OpenData>

// Direct setter (returns WrongState error if not in this state)
pub fn set_open_data(&mut self, data: OpenData) -> Result<(), DynamicError>
```

**Example: Circuit Breaker Pattern**

```rust,ignore
use std::time::Instant;

#[derive(Debug, Clone)]
struct OpenData {
    opened_at: Instant,
    failure_count: u32,
}

#[derive(Debug, Clone)]
struct HalfOpenData {
    consecutive_successes: u32,
}

state_machine! {
    name: Circuit,
    dynamic: true,
    initial: Closed,
    states: [
        Closed,
        Open(OpenData),
        HalfOpen(HalfOpenData),
    ],
    events {
        trip { transition: { from: Closed, to: Open } }
        attempt_reset { transition: { from: Open, to: HalfOpen } }
        reset { transition: { from: HalfOpen, to: Closed } }
        fail_again { transition: { from: HalfOpen, to: Open } }
    }
}

struct CircuitBreaker {
    machine: DynamicCircuit,
}

impl CircuitBreaker {
    pub fn new() -> Self {
        Self {
            machine: DynamicCircuit::new(()),
        }
    }

    pub fn call(&mut self) -> Result<Response, Error> {
        match self.machine.current_state() {
            CircuitState::Closed => {
                // Execute call
                match execute_request() {
                    Ok(resp) => Ok(resp),
                    Err(e) => {
                        // Trip circuit on failure
                        self.machine.handle(CircuitEvent::Trip).unwrap();
                        self.machine
                            .set_open_data(OpenData {
                                opened_at: Instant::now(),
                                failure_count: 1,
                            })
                            .unwrap();
                        Err(e)
                    }
                }
            }
            CircuitState::Open => {
                // Check if timeout expired
                if let Some(data) = self.machine.open_data() {
                    if data.opened_at.elapsed() > Duration::from_secs(60) {
                        // Try half-open
                        self.machine.handle(CircuitEvent::AttemptReset).unwrap();
                        self.machine
                            .set_half_open_data(HalfOpenData {
                                consecutive_successes: 0,
                            })
                            .unwrap();
                        return self.call(); // Retry
                    }
                }
                Err(Error::CircuitOpen)
            }
            CircuitState::HalfOpen => {
                // Execute call, track successes
                match execute_request() {
                    Ok(resp) => {
                        // Increment success counter
                        if let Some(data) = self.machine.half_open_data_mut() {
                            data.consecutive_successes += 1;

                            // Reset after 3 successes
                            if data.consecutive_successes >= 3 {
                                self.machine.handle(CircuitEvent::Reset).unwrap();
                            }
                        }
                        Ok(resp)
                    }
                    Err(e) => {
                        // Back to Open
                        self.machine.handle(CircuitEvent::FailAgain).unwrap();
                        Err(e)
                    }
                }
            }
        }
    }
}
```

**Key Points:**

- **Read accessors** return `Option<&T>` - None when not in that state
- **Mutable accessors** return `Option<&mut T>` - allows in-place updates
- **Setters** return `Result<(), DynamicError>` - errors with `WrongState` if not in target state
- Works seamlessly with hierarchical states (substates can access parent state data)
- Zero overhead - delegates directly to typestate machine's field access

### Performance Considerations

| Mode | Overhead | Safety | Use Case |
|------|----------|--------|----------|
| **Typestate** | Zero (PhantomData) | Compile-time | Known sequences |
| **Dynamic** | Enum match (~few ns) | Runtime | Event-driven |

Dynamic mode adds minimal runtime overhead (enum discriminant check + match). For most applications, this is negligible compared to the actual business logic.

### Design Philosophy

This library provides **both** modes:
- **Typestate by default** – Zero-cost abstractions, compile-time safety
- **Dynamic opt-in** – Runtime flexibility when needed
- **Seamless conversion** – Switch modes as requirements change

You're never forced to choose one over the other. Start with typestate for safety, convert to dynamic for flexibility, and back again when you need guarantees.

---

## Comparison to Ruby's state_machines

If you're coming from Ruby, here's how the concepts map:

### Ruby
```ruby
class Vehicle
  state_machine :state, initial: :parked do
    event :ignite do
      transition parked: :idling
    end

    before_transition parked: :idling, do: :check_fuel
  end

  def check_fuel
    puts "Checking fuel..."
  end
end

# Usage
vehicle = Vehicle.new
vehicle.ignite  # Mutates vehicle in place
```

### Rust (Typestate)
```rust
use state_machines::state_machine;

state_machine! {
    name: Vehicle,

    initial: Parked,
    states: [Parked, Idling],
    events {
        ignite {
            before: [check_fuel],
            transition: { from: Parked, to: Idling }
        }
    }
}

impl<C, S> Vehicle<C, S> {
    fn check_fuel(&self) {
        println!("Checking fuel...");
    }
}

fn main() {
    // Type: Vehicle<Parked>
    let vehicle = Vehicle::new(());

    // Type: Vehicle<Idling>
    let vehicle = vehicle.ignite().unwrap();
}
```

**Key Differences:**
- **Typestate pattern**: Each state is encoded in the type system (`Vehicle<Parked>` vs `Vehicle<Idling>`)
- **Move semantics**: Transitions consume the old state and return a new one
- **Compile-time validation**: Can't call `ignite()` twice - second call won't compile!
- **Zero overhead**: PhantomData optimizes away completely
- **Explicit errors**: Guards return `Result<Machine<NewState>, (Machine<OldState>, GuardError)>`
- **No mutation**: Callbacks take `&self`, not `&mut self` (machine is consumed by transition)

---

## `no_std` Support

Works on embedded targets like ESP32:

```rust,ignore
#![no_std]

use state_machines::state_machine;

state_machine! {
    name: LedController,

    initial: Off,
    states: [Off, On, Blinking],
    events {
        toggle { transition: { from: Off, to: On } }
        blink { transition: { from: On, to: Blinking } }
    }
}

fn embedded_main() {
    // Type: LedController<Off>
    let led = LedController::new(());

    // Type: LedController<On>
    let led = led.toggle().unwrap();

    // Type: LedController<Blinking>
    let led = led.blink().unwrap();

    // Wire up to GPIO pins...
}
# fn main() {} // For doctest
```

- Disable default features: `state-machines = { version = "0.21", default-features = false }`
- The library uses no allocator - purely stack-based with zero-sized state markers
- CI runs `cargo build --no-default-features` to prevent std regressions
- See `examples/no_std_flight/` for a complete embedded example

---

## Performance

This library achieves **true zero-cost abstractions** for typestate mode:

| Feature | Overhead | Notes |
|---------|----------|-------|
| **Typestate mode** | | |
| Guards | ~0 ps | Compiled to inline comparisons |
| Callbacks | ~0 ps | Compiled to inline function calls |
| Around callbacks | ~0 ps | Compiled to inline function calls |
| Hierarchical transitions | ~3-4 ns | Minimal cost for storage lifecycle |
| State data access | ~1 ns | Direct field access |
| **Dynamic mode** | | |
| Event dispatch | ~few ns | Enum match + method call |
| State introspection | ~0 ps | Direct field access |

Guards, callbacks, and around callbacks in typestate mode add **literally zero runtime overhead** - the compiler optimizes them completely. Dynamic mode adds minimal overhead (enum matching), typically under 10ns per transition.

Run benchmarks yourself:
```bash
cargo bench --bench typestate_transitions
```

---

## Documentation

### Interrupted dynamic dispatch

Cancelling an async `handle()` future after it has started, or unwinding through
a callback, poisons its wrapper: the owned in-flight machine cannot be recovered
without imposing `Clone`. `is_poisoned()` reports this, subsequent dispatch and
setters return `DynamicError::Poisoned`, and availability/completion queries are
empty. `current_state()` is the last committed state, not a live state when
poisoned. Replace the wrapper with a fresh/restored machine. Use
`try_into_snapshot()` when interruption is possible; the legacy `into_snapshot()`
panics on poison. Dropping an unpolled future does not poison the machine.

### State-data accessor migration

Typed `state_name_data()` / `state_name_data_mut()` now return `Option<&T>` /
`Option<&mut T>`, just like dynamic accessors. This is an API change: initial
construction and restore may legitimately leave active data absent, so being
in the right state alone cannot guarantee data exists. Match on the option, or
use `.expect("initialized on entry")` when your application enforces that invariant.

### Explicit startup and owned entry data

Concrete `context:` declarations no longer generate `Default` for the dynamic
wrapper: construct with `new(context)` or implement `Default` explicitly when
appropriate. This permits owned configuration/resources without a Default
implementation. Generic-context wrappers retain `Default` when `C: Default`.
Generated dynamic state selector enums implement `Default` as their declared
initial leaf, independently of context construction.

`Machine::new(ctx).initialize()` runs active entry hooks outer-to-inner and returns
the source machine on a fallible hook error. `DynamicMachine::initialize(ctx)` is the
runtime equivalent. Both are async for async machines. Call startup once on a
fresh machine; neither constructors nor restore run entry hooks implicitly.
Supply initial data with `.with_state_name_data(owned_value)`.

For transition entry, `transition: { from: Idle, to: Active, data: make_resource }`
uses a factory instead of `Default`. The source-machine factory returns Active's
data and takes `&mut Payload` (or no argument without a payload); it can move
resources using `Option::take()` without Clone. It runs after exit hooks and
before target entry hooks. After callbacks see the remaining payload. Factories
must target data-carrying leaves and support async machines.

### Hierarchical final completion

`final_states` accepts leaves and composites. A final leaf completes its parent;
if that parent is declared final, completion propagates bottom-up to its parent,
and ultimately the machine when the root child is final. A non-final composite
stops propagation; its `completion: Parent` edge can advance the workflow.
Completion hooks/signals follow bottom-up order, irrespective of declaration
order. Internal transitions do not repeat them. Final scopes cannot declare
their own outgoing edges; inherited exits remain available.

### Composite transition kinds

`kind: internal` is the targetless form of `internal: true`. `kind: local`
requires a superstate source and stays within it, preserving the parent.
`kind: external` exits and re-enters the declared source scope even when its
target is inside that scope: parent entry/exit hooks run and parent data resets.
History records successful re-entry exits too. Omit `kind` to retain legacy
common-ancestor-preserving behavior; external leaf self-transitions still reset
the leaf. Callback failures recover old control state/data, not external effects.

### Hierarchical event precedence

Set event `hierarchical: true` to select the deepest enabled declared source
first, falling back through ancestors when child guards reject. Declaration
order does not let a parent shadow a child. A guardless parent handler is a
natural fallback. Same-scope overlaps are rejected; `hierarchical` and ordered
`branching` are mutually exclusive. Event guards run once, and only the selected
transition runs callbacks. Both typestate outcomes and dynamic dispatch share
this policy; events without this option retain existing behavior.

### Eventless transitions

Event `automatic: true` names an eventless trigger for inspection; it needs no
external event and cannot have a payload. `dynamic.stabilize(max_steps)` selects
enabled automatic edges in declaration order until stable. Event/candidate guards
are evaluated once per selection, then only the chosen edge runs callbacks.
Branching, hierarchical selection, history, factories, and async hooks still work.
After ordinary `handle()` succeeds, machines with automatic edges stabilize with
a 64-microstep budget. Explicitly call `stabilize()` after construction/restore;
those remain inert. Cycles return `DynamicError::StepLimit` with the last committed
state intact. A failed selected action propagates, not falls back to another edge.
Typestate methods remain explicit; use dynamic mode to follow runtime-dependent
automatic chains.

### Parent completion transitions

Event `completion: Processing` fires automatically only when Processing's
immediate child is a declared final leaf. Its transitions must use
`from: Processing` and cannot be internal or require a payload. Completion hooks
and notifications run before the parent advances; nested parent completions use
the same bounded stabilization loop. Final leaves cannot declare their own exits,
but inherited parent exits are legal. An explicit typed completion method is
available only on the appropriate final leaves. Constructors/restore remain inert.

### Optional event runner

Enable `runtime` for `runtime::Runner::new(dynamic_machine, capacity)` and the
executor-independent `Machine` adapter. `enqueue()` is external FIFO; `raise()` is
internal FIFO with priority. Cloned `runner.sink()` handles let callbacks raise
events without re-entering dispatch. `defer_in(state, matches)` and
`defer_while(scope_predicate, matches)` explicitly hold matching events until the
scope exits, then recall them FIFO ahead of queued external events.

`drain(max_steps).await` is bounded and counts deferrals as steps. Capacity includes
deferred events; full/closed errors return the original event without Clone.
Dispatch errors consume the attempted event but preserve the remaining queue.
Extraction/drop closes the mailbox. The runner is single-executor (`Rc`), uses
`no_std` + `alloc`, and creates no threads. Queues are not part of FSM snapshots.
Enable `runtime-send` for a `std`-backed, thread-safe mailbox and Send dispatch,
automatic-step and activity futures. The same driver owns and dispatches the
machine; cloned sinks may enqueue owned events from other threads, and runners
and child/native-region compositions may move into `tokio::spawn`. This adds no
executor, threads, or parallel dispatch. Events, state, errors and machines must
be Send; async guards/callbacks must yield Send futures. Generic generated async
adapters additionally require Sync contexts; synchronous adapters only need Send.
Activities still cancel by
dropping their futures, not by aborting detached tasks. Mailbox guards do not
span dispatch, polling, event destruction, waker callbacks or debug output.

`runtime-send` selects the runtime contract for the linked facade. Cargo feature
unification means it applies to every consumer of that facade in a build, not
just the crate requesting it. For non-Send contexts/resources or no-std targets,
use `runtime` without `runtime-send`; CI tests both contracts separately.

Dynamic `transition_epoch()` advances on committed external edges, including
self-re-entry, but not internal edges; conversions/restore reset this runtime tag.
With `runtime`, custom error types on public generated machines must also be
public, because they are exposed as the adapter's associated error type.

### State-scoped deadlines

`runner.schedule_after(&clock, delay, event)` reserves mailbox capacity and
returns a cancellable `TimerId`. Implement `runtime::Clock::now()` in monotonic
`u64` ticks; the host calls `tick(&clock)` to enqueue due timeouts, then `drain`.
`next_deadline()` helps integrate an executor's timer driver. No wall clock,
sleeping task, or executor dependency is built in.

Timers belong to the current leaf **visit**: internal transitions retain them;
external transitions (including self-re-entry) cancel them. The visit is checked
again at delivery, so a queued timeout cannot leak into a new visit. Equal
deadlines use scheduling order; due events join the external FIFO. Cancellation
of an already queued/deferred event releases capacity when `drain` skips it.
Scheduling errors return the original owned event; backwards clocks and deadline
overflow are rejected. Timers, queues and runtime epochs are not persisted snapshots.

Use `schedule_after_in(WorkScope::Named("Running"), &clock, delay, event)`,
`invoke_future_in(scope, future)` or `invoke_child_in(scope, ...)` for a composite
visit instead. Work then survives sibling/local transitions, but exits and
external re-entry invalidate it—even if automatic steps return to the same leaf.
Generated `scope_epoch(name)` queries active leaf/composite visits; inactive names
return `None`. An inactive invocation returns the original event/future/child.

### Activities and invoked child machines

`runner.invoke_future(future)` owns and polls a future whose output is a parent
event. Map an operation's `Result` to done/error events inside that future.
Completion enters the internal queue exactly once; each invocation reserves one
mailbox slot. `cancel_activity(id)` or exit/re-entry of the invoking leaf drops
the future and suppresses stale queued completion events. Internal edges retain
activities. This does **not** promise to abort detached executor tasks merely
because their join handle was dropped.

`invoke_child(child_runner, batch_limit, on_done, on_error)` returns an activity
ID and the child's owned-event sink. A child final state maps to `on_done(child)`;
a dispatch failure maps to `on_error(error, child)`. Both recover the owned child
machine. Completion/cancellation closes its channel; full/invalid invocation
returns the unpolled future or child to the caller.

`drain` polls pending activities without waiting for them. Use
`wait_for_work().await` to sleep on real mailbox/activity wakers, then drain.
Child batches yield cooperatively at their step limit. There are no spawned
tasks or threads; the host drives this single-executor runner. Invocations are
runtime operations by default; named scopes and declarative entry rules also
support composite lifetimes. Pending execution is not part of snapshots.

### Declarative runtime lifecycle

Enable `runtime`, opt into dynamic mode, and declare scoped entry rules:

```text
runtime: {
    Running {
        after: [{ delay: 10, event: make_timeout }],
        invoke: [start_operation],
        defer: [load],
    }
}
```

The event factory returns an owned generated event; an invocation factory returns
an owned `'static` future yielding a generated done/error event. For child machines,
reuse `runtime::run_child(child_runner, batch_limit)` inside that future.
Factories are ordinary methods on the typed machine, like existing callbacks.
`defer` names external events and holds their owned payloads until the scope exits.

Call `runner.start(&clock)` for initial entry setup. Constructors/restore remain
inert. Later committed entries—including transient automatic microsteps—are wired
by `drain`; the runner reuses the generated edge selector, not a second transition
engine. A parent declaration survives sibling/local transitions and restarts on
external re-entry. Entry setup uses the latest host-observed clock: call `tick`
before dispatch to advance time. Zero delays queue immediately.

Setup reserves capacity **before** calling each factory. `RunError::Setup` reports
missing clocks, overflow or backpressure; already installed rules are not repeated
on retry. `set_capacity` can expand a full mailbox without losing events. Factories
must not rely on I/O compensation if their own code panics. Root automatic cycles
return `RunError::AutomaticStepLimit`, distinct from the queued-event budget.
Runtime declarations are included in the inspectable schema.

Explicit `Runner::drain` also settles the initial automatic configuration before
processing queued events. This lets eventless-only children/regions finish
without waiting for a mailbox event. Construction and restore remain inert;
automatic cycles use the same bounded stabilization error.

### Orthogonal regions, fork and join

`runtime::Parallel::new(left, right)` composes independent machines with a tuple
active state. Nest it for more than two regions. `ParallelEvent::Left`/`Right`
route to one region; `Both { left, right }` or `fork(left, right).await` route
separate owned events to both, without requiring cloned payloads.

This is deterministic **logical** parallelism: left dispatch precedes right,
including async effects. If right rejects, `ParallelError::Right` reports whether
left dispatched successfully; committed effects are not rolled back. Completion
requires every region to finish; `take_join()` returns each newly completed
configuration once. Construction from already-final regions remains inert.

`Parallel` implements `Machine`, so it can use a runner or be invoked as a child.
`into_regions()` recovers ownership. There is no cross-region transaction or
automatic conflict arbitration.

### Native named orthogonal regions

With `runtime`, the same macro can compose two or more existing dynamic machines:

```ignore
state_machine! {
    name: Session,
    regions: { network: DynamicNetwork<()>, auth: DynamicAuth<()> },
    events {
        open {
            payload: (NetworkRequest, Credentials),
            routes: {
                network: NetworkEvent::Connect(payload.0),
                auth: AuthEvent::Login(payload.1),
            }
        }
    }
}
let mut session = Session::new(network, auth, 32);
session.start(&clock)?;
session.handle(SessionEvent::Open((request, credentials))).await?;
let configuration = session.current_state(); // SessionState { network, auth }
```

Routes explicitly split owned payloads without cloning. Dispatch follows region
declaration order, including partial errors; unattempted route values are dropped
on failure, not falsely reported as rolled back. Each region owns the existing
`Runner`, so declarative deferral, deadlines, activities and automatic microsteps
use the same driver. `start`, `tick`, `poll_activities` and `drain` drive all regions,
including nested native compositions. Budgets are **per region**, not a global
cross-region microstep budget. `take_join` returns a named configuration once on
all-regions-finished; construction is inert.

Named accessors (`network()` / `network_mut()`) expose region runners and sinks.
After driving one region directly, call the composition's `drain` to observe joins.
Named work scopes use qualified paths such as `"network/Connecting"`; `"network"`
is the continuously active region, not a leaf visit. Schema metadata records
region names/types and common-event routes. Start a native composition explicitly
before enclosing it in an ordinary runner or invoking it as a child; its region
clocks remain host-driven.
- **[API Docs](https://docs.rs/state-machines)** – Full API reference
- **[Crates.io](https://crates.io/crates/state-machines)** – Published crate versions
- **[GitHub](https://github.com/state-machines/state-machines-rs)** – Source code and issues

---

### Unified region snapshots and history

Enable `serde` and set `snapshot: true` on native region declarations and their
child machines. Capture returns one versioned owned envelope:

```ignore
let snapshot = session.try_into_snapshot().ok().unwrap();
let json = serde_json::to_string(&snapshot)?;
let snapshot: SessionSnapshot = serde_json::from_str(&json)?;
let restored = Session::from_snapshot(snapshot, 32).ok().unwrap();
```

Each named field contains that region's existing context, active data and
shallow/deep history. No `Clone` is required. The common `runtime::SnapshotMachine`
trait also snapshots recursively nested `Parallel` adapters. All headers, states,
active-data ownership and histories are validated **before consuming any region**.
A failed restore returns the entire original envelope intact for migration.
Serde rejects unknown/missing region keys.

Restore is inert: no startup hooks, automatic progression or synthetic join.
Visit generations reset. Mailboxes, deferred/queued events, timers, activity
futures, wakers and old capacity are **not** persisted. Capture closes old region
channels and drops their ephemeral work; restore uses the supplied fresh mailbox
capacity. Explicitly `start` restored region runtime work with the host's clock.
History restores control state, not previously suspended resources.

### Running the examples

Examples live in their own Cargo workspace. They stay `publish = false` and are
excluded from release-please's workspace graph/manifest, so library releases do
not bump their versions or create example changelogs.

```sh
cargo run --manifest-path examples/Cargo.toml -p traffic_light
cargo test --manifest-path examples/Cargo.toml --workspace --all-features
cargo build --manifest-path tests/feature_split/Cargo.toml
```

CI tests, lints and runs these examples separately from the release workspace.

The existing programs include these runnable statechart scenarios:

| Program (`-p`) | New scenario |
| --- | --- |
| `basic_transitions` | Explicit startup, optional initial data and owned entry factories without `Clone`/`Default` on the resource |
| `traffic_light` | Logical-clock deadlines, composite timer retention, raised events, owned deferral/recall and external reset |
| `hierarchical_thinking` | Local/external domains, child-first fallback, eventless boot and bottom-up final-composite completion |
| `async_patterns` | Owned child activity, mailbox wakeups, sibling retention, reset cancellation and ownership-preserving retry |
| `dynamic_dispatch_when` | Native named regions, owned common routes, partial fork errors, unified snapshots, independent deep/shallow history and one-shot join |

These scenarios use assertions in the executable and reuse/reset machines for
repeat passes; they do not duplicate the crate's unit tests. Async simulated I/O
yields cooperatively and deadline examples use logical ticks, not sleeps.
`callbacks_lifecycle` and `guards_and_validation` retain their focused callback/
guard walkthroughs; `no_std_flight` remains an allocation-free embedded library.

## Contributing

Contributions are welcome! This is a learning project, so:

1. **Keep comments** – Explain *why*, not just *what*
2. **Show Rust idioms** – If something is unidiomatic, fix it *and document the correct pattern*
3. **Test thoroughly** – Run `cargo test --workspace` and
   `cargo test --manifest-path examples/Cargo.toml --workspace --all-features`.
4. **Compare to Ruby** – If you're changing behavior, note how it differs from the Ruby gem

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

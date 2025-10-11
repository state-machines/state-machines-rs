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

✅ **Typestate Pattern** – Compile-time state safety using Rust's type system with zero runtime overhead

✅ **Guards & Unless** – Conditional transitions at event and transition levels

✅ **Callbacks** – `before`/`after` hooks at event level

✅ **Async Support** – First-class `async`/`await` for guards and callbacks

✅ **Event Payloads** – Pass data through transitions with type-safe payloads

✅ **No-std Compatible** – Works on embedded targets (ESP32, bare metal)

✅ **Type-safe** – Invalid transitions become compile errors, not runtime errors

🚧 **Hierarchical States** – Planned for future release

🚧 **Around Callbacks** – Planned for future release

🚧 **Dynamic Dispatch** – Event-driven mode planned (see `docs/dual_mode_design.md`)

---

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
state-machines = "0.1"
```

### Basic Example

```rust
use state_machines::state_machine;

// Define your state machine
state_machine! {
    name: TrafficLight,
    state: LightState,
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
    let light = TrafficLight::new();
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
    state: DoorState,
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

impl<S> Door<S> {
    fn path_clear(&self) -> bool {
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
    let door = Door::new();
    let door = door.open().unwrap();
    let door = door.close().unwrap();

    // Failed guard check
    DOOR_OBSTRUCTED.store(true, Ordering::Relaxed);
    let err = door.open().expect_err("should fail when obstructed");
    let (_door, guard_err) = err;
    assert_eq!(guard_err.guard, "path_clear");
}
```

### Async Support

The typestate pattern works seamlessly with async Rust:

```rust,ignore
use state_machines::state_machine;

state_machine! {
    name: HttpRequest,
    state: RequestState,
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

impl<S> HttpRequest<S> {
    async fn has_network(&self) -> bool {
        // Async guard checks network availability
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        true
    }
}

#[tokio::main]
async fn main() {
    // Type: HttpRequest<Idle>
    let request = HttpRequest::new();

    // Type: HttpRequest<Pending>
    let request = request.send().await.unwrap();

    // Type: HttpRequest<Success>
    let request = request.succeed().await.unwrap();
}
```

### Event Payloads

```rust
use state_machines::state_machine;

#[derive(Clone)]
struct LoginCredentials {
    username: String,
    password: String,
}

state_machine! {
    name: AuthSession,
    state: SessionState,
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

impl<S> AuthSession<S> {
    fn valid_credentials(&self, creds: &LoginCredentials) -> bool {
        // Guard receives payload reference
        creds.username == "admin" && creds.password == "secret"
    }
}

fn main() {
    let session = AuthSession::new();
    // Type is AuthSession<LoggedOut>

    let good_creds = LoginCredentials {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    let session = session.login(good_creds).unwrap();
    // Type is AuthSession<LoggedIn>
}
```

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
    state: VehicleState,
    initial: Parked,
    states: [Parked, Idling],
    events {
        ignite {
            before: [check_fuel],
            transition: { from: Parked, to: Idling }
        }
    }
}

impl<S> Vehicle<S> {
    fn check_fuel(&self) {
        println!("Checking fuel...");
    }
}

fn main() {
    // Type: Vehicle<Parked>
    let vehicle = Vehicle::new();

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
    state: LedState,
    initial: Off,
    states: [Off, On, Blinking],
    events {
        toggle { transition: { from: Off, to: On } }
        blink { transition: { from: On, to: Blinking } }
    }
}

fn embedded_main() {
    // Type: LedController<Off>
    let led = LedController::new();

    // Type: LedController<On>
    let led = led.toggle().unwrap();

    // Type: LedController<Blinking>
    let led = led.blink().unwrap();

    // Wire up to GPIO pins...
}
# fn main() {} // For doctest
```

- Disable default features: `state-machines = { version = "0.1", default-features = false }`
- The library uses no allocator - purely stack-based with zero-sized state markers
- CI runs `cargo build --no-default-features` to prevent std regressions
- See `examples/no_std_flight/` for a complete embedded example

---

## Documentation

- **[API Docs](https://docs.rs/state-machines)** – Full API reference
- **[Migration Notes](docs/migration_notes.md)** – Compatibility guidance for pre-hierarchy adopters

---

## Migration Notes (High Level)

- Existing flat machines continue to compile unchanged; the macro infers leaf states exactly as before.
- New superstate syntax only adds enum variants—no generated method signatures were removed.
- Enable the `std` feature (default) if you rely on `std` types in guards/actions; disable it for embedded builds.

---

## Contributing

Contributions are welcome! This is a learning project, so:

1. **Keep comments** – Explain *why*, not just *what*
2. **Show Rust idioms** – If something is unidiomatic, fix it *and document the correct pattern*
3. **Test thoroughly** – All tests must pass (`cargo test --workspace`)
4. **Compare to Ruby** – If you're changing behavior, note how it differs from the Ruby gem

---

## License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

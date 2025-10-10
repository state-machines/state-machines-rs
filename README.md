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
  - Hierarchical states (superstates)
  - Guards and unless conditions
  - Before/after/around callbacks
  - Sync and async support
  - `no_std` compatibility (I want to use it in my embedded systems)
  - Payload support for event data
  - Compile-time validation

### For the Rust Community

**You're welcome to open PRs** to fix fundamentally wrong Rust concepts—but please **don't remove comments just because "we know it"**. This codebase serves beginners. If something can be explained better, improve the comment. If a pattern is unidiomatic, fix it *and document why*.

---

## Features

✅ **Hierarchical States** – Superstates with automatic event bubbling and state-local storage

✅ **Guards & Unless** – Conditional transitions at event and transition levels

✅ **Callbacks** – `before`/`after`/`around` hooks with flexible filters

✅ **Async Support** – First-class `async`/`await` for guards, actions, and callbacks

✅ **Event Payloads** – Pass data through transitions with type-safe payloads

✅ **No-std Compatible** – Works on embedded targets (ESP32, bare metal)

✅ **Introspection** – Runtime metadata for debugging and visualization

✅ **Type-safe** – Compile-time validation of states, events, and transitions

---

## Quick Start

Add to your `Cargo.toml`:

```toml
[dependencies]
state-machines = "0.1"
```

### Basic Example

```rust
use state_machines::{state_machine, TransitionResult};

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
    let mut light = TrafficLight::new();
    assert_eq!(light.state(), LightState::Red);

    light.next().unwrap();
    assert_eq!(light.state(), LightState::Green);

    light.next().unwrap();
    assert_eq!(light.state(), LightState::Yellow);
}
```

### With Guards and Callbacks

```rust
use state_machines::state_machine;

state_machine! {
    name: Door,
    state: DoorState,
    initial: Closed,
    states: [Closed, Open, Locked],
    events {
        open {
            guards: [not_locked],
            before: [check_safety],
            after: [log_opened],
            transition: { from: Closed, to: Open }
        }
        close {
            transition: { from: Open, to: Closed }
        }
        lock {
            transition: { from: Closed, to: Locked }
        }
    }
}

impl Door {
    fn not_locked(&self) -> bool {
        self.state() != DoorState::Locked
    }

    fn check_safety(&mut self) {
        println!("Checking if path is clear...");
    }

    fn log_opened(&mut self) {
        println!("Door opened at {:?}", std::time::SystemTime::now());
    }
}

fn main() {
    let mut door = Door::new();

    // Try to open - succeeds
    door.open().unwrap();
    assert_eq!(door.state(), DoorState::Open);

    door.close().unwrap();
    door.lock().unwrap();

    // Try to open while locked - fails
    assert!(door.open().is_err());
}
```

### Hierarchical States (Superstates)

```rust
use state_machines::state_machine;

#[derive(Default, Debug)]
struct FlightData {
    altitude: u32,
    speed: u32,
}

state_machine! {
    name: Aircraft,
    state: AircraftState,
    initial: Grounded,
    states: [
        Grounded,
        superstate InFlight {
            state Climbing(FlightData),
            state Cruising(FlightData),
            state Descending(FlightData),
        },
    ],
    events {
        takeoff {
            transition: { from: Grounded, to: InFlight }
        }
        land {
            transition: { from: InFlight, to: Grounded }
        }
        level_off {
            transition: { from: Climbing, to: Cruising }
        }
    }
}

fn main() {
    let mut plane = Aircraft::new();

    plane.takeoff().unwrap();
    // Enters InFlight superstate's initial child: Climbing
    assert_eq!(plane.state(), AircraftState::Climbing);

    // Access state-local data
    if let Some(data) = plane.climbing_data_mut() {
        data.altitude = 30000;
        data.speed = 500;
    }

    plane.level_off().unwrap();
    assert_eq!(plane.state(), AircraftState::Cruising);

    // Can transition from any child of InFlight
    plane.land().unwrap();
    assert_eq!(plane.state(), AircraftState::Grounded);
}
```

### Async Support

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

impl HttpRequest {
    async fn has_network(&self) -> bool {
        // Async guard checks network availability
        tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        true
    }
}

#[tokio::main]
async fn main() {
    let mut request = HttpRequest::new();

    request.send().await.unwrap();
    assert_eq!(request.state(), RequestState::Pending);

    request.succeed().await.unwrap();
    assert_eq!(request.state(), RequestState::Success);
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

impl AuthSession {
    fn valid_credentials(&self, creds: &LoginCredentials) -> bool {
        // Guard receives payload reference
        creds.username == "admin" && creds.password == "secret"
    }
}

fn main() {
    let mut session = AuthSession::new();

    let good_creds = LoginCredentials {
        username: "admin".to_string(),
        password: "secret".to_string(),
    };

    session.login(good_creds).unwrap();
    assert_eq!(session.state(), SessionState::LoggedIn);
}
```

---

## Comparison to Ruby's state_machines

If you're coming from Ruby, here's how the concepts map:

### Ruby
```ruby,ignore
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
```

### Rust
```rust,ignore
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
    },
    callbacks {
        before_transition [
            { name: check_fuel, from: Parked, to: Idling }
        ]
    }
}

impl Vehicle {
    fn check_fuel(&mut self) {
        println!("Checking fuel...");
    }
}
```

**Key Differences:**
- Rust uses a **macro DSL** instead of class-level DSL
- **Ownership matters**: `&mut self` for transitions, `&self` for guards
- **Type safety**: States are enums, not symbols
- **No implicit state storage**: Use state-local data syntax explicitly
- **Compile-time validation**: Invalid transitions fail at compile time

---

## `no_std` Support

Works on embedded targets like ESP32:

```rust,ignore
#![no_std]
#![no_main]

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

#[entry]
fn main() -> ! {
    let mut led = LedController::new();
    led.toggle().unwrap();
    // Wire up to GPIO pins...
    loop {}
}
```

---

## Documentation

- **[API Docs](https://docs.rs/state-machines)** – Full API reference

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

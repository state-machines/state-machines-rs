#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

#![no_std]

use core::sync::atomic::{AtomicBool, Ordering};
use state_machines::state_machine;

#[derive(Default, Debug)]
pub struct Checklist {
    #[allow(dead_code)]
    armed: bool,
}

#[derive(Default, Debug)]
pub struct EnginesReady {
    #[allow(dead_code)]
    thrust_percentage: u8,
}

state_machine! {
    name: FlightController,
    initial: Idle,
    states: [
        Idle,
        superstate Armed(Checklist) {
            state ChecklistComplete(Checklist),
            state EnginesPrimed(EnginesReady),
        },
        Ascending,
    ],
    events {
        arm {
            guards: [power_is_present],
            transition: { from: Idle, to: Armed }
        }
        verify {
            transition: { from: ChecklistComplete, to: EnginesPrimed }
        }
        launch {
            transition: { from: Armed, to: Ascending }
        }
        abort {
            transition: { from: Armed, to: Idle }
        }
    }
}

static POWER_FLAG: AtomicBool = AtomicBool::new(false);

impl<C, S> FlightController<C, S> {
    /// Pretend to flip a hardware latch that enables power for pre-flight checks.
    pub fn connect_power_supply(&self) {
        POWER_FLAG.store(true, Ordering::Relaxed);
    }

    /// Guard used by the `arm` event.
    fn power_is_present(&self, _ctx: &C) -> bool {
        POWER_FLAG.load(Ordering::Relaxed)
    }
}

// Note: Superstate data access methods like `checklist_complete_data_mut()` and
// `engines_primed_data_mut()` are not yet fully implemented in the typestate API.
// These would be used to access state-local storage for hierarchical states.
//
// When superstates are fully supported, you would access them like:
//
//   impl FlightController<ChecklistComplete> {
//       pub fn set_armed(&mut self, armed: bool) {
//           // Access ChecklistComplete state data here
//       }
//   }
//
//   impl FlightController<EnginesPrimed> {
//       pub fn set_thrust(&mut self, thrust: u8) {
//           // Access EnginesPrimed state data here
//       }
//   }

// Note: The typestate pattern requires consuming and returning the machine at each step.
// The old runtime-based helper functions have been removed as they don't fit this pattern.
//
// Example usage with typestate:
//
//   let controller = FlightController::new(());  // Type: FlightController<(), Idle>
//   controller.connect_power_supply();
//   let controller = controller.arm().unwrap(); // Type: FlightController<(), ChecklistComplete>
//   let controller = controller.verify().unwrap(); // Type: FlightController<(), EnginesPrimed>
//   let controller = controller.launch().unwrap(); // Type: FlightController<(), Ascending>

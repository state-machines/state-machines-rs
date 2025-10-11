use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::state_machine;

static CAPTAIN_ON_BRIDGE: AtomicBool = AtomicBool::new(false);
static SOLUTION_VALID: AtomicBool = AtomicBool::new(false);
static DIAGNOSTICS_PASSING: AtomicBool = AtomicBool::new(false);
static NAV_TEST_LOCK: Mutex<()> = Mutex::new(());

state_machine! {
    name: NavigationComputer,
    state: NavState,
    initial: Idle,
    states: [Idle, Plotting, CourseLocked, Replotting, CriticalFailure],
    events {
        plot_course {
            transition: { from: Idle, to: Plotting }
            transition: { from: Replotting, to: Plotting }
        }
        lock_course {
            guards: [captain_on_bridge]
            transition: { from: Plotting, to: CourseLocked, guards: [solution_valid] }
        }
        request_replot {
            transition: { from: CourseLocked, to: Replotting, guards: [diagnostics_passing] }
        }
        fail_safe {
            transition: { from: [Idle, Plotting, CourseLocked, Replotting], to: CriticalFailure }
        }
    }
}

impl<S> NavigationComputer<S> {
    fn set_captain_on_bridge(value: bool) {
        CAPTAIN_ON_BRIDGE.store(value, Ordering::SeqCst);
    }

    fn set_solution_valid(value: bool) {
        SOLUTION_VALID.store(value, Ordering::SeqCst);
    }

    fn set_diagnostics_passing(value: bool) {
        DIAGNOSTICS_PASSING.store(value, Ordering::SeqCst);
    }

    fn captain_on_bridge(&self) -> bool {
        CAPTAIN_ON_BRIDGE.load(Ordering::SeqCst)
    }

    fn solution_valid(&self) -> bool {
        SOLUTION_VALID.load(Ordering::SeqCst)
    }

    fn diagnostics_passing(&self) -> bool {
        DIAGNOSTICS_PASSING.load(Ordering::SeqCst)
    }
}

#[test]
fn lock_course_requires_event_and_transition_guards() {
    let _guard = NAV_TEST_LOCK.lock().unwrap();
    NavigationComputer::<Idle>::set_captain_on_bridge(false);
    NavigationComputer::<Idle>::set_solution_valid(false);

    let nav = NavigationComputer::new();
    let nav = nav.plot_course().expect("plotting should start from Idle");
    // Type is NavigationComputer<Plotting>

    let auth_error = nav
        .lock_course()
        .expect_err("cannot lock course without the captain present");
    let (nav, guard_err) = auth_error;
    assert_eq!(guard_err.guard, "captain_on_bridge");
    assert_eq!(guard_err.event, "lock_course");

    NavigationComputer::<Idle>::set_captain_on_bridge(true);
    let solution_error = nav
        .lock_course()
        .expect_err("solution must be valid before locking course");
    let (nav, guard_err) = solution_error;
    assert_eq!(guard_err.guard, "solution_valid");
    assert_eq!(guard_err.event, "lock_course");

    NavigationComputer::<Idle>::set_solution_valid(true);
    let nav = nav
        .lock_course()
        .expect("lock_course should succeed once all guards pass");
    // Type is NavigationComputer<CourseLocked>
}

#[test]
fn replot_requires_transition_guard() {
    let _guard = NAV_TEST_LOCK.lock().unwrap();
    NavigationComputer::<Idle>::set_captain_on_bridge(true);
    NavigationComputer::<Idle>::set_solution_valid(true);
    NavigationComputer::<Idle>::set_diagnostics_passing(false);

    let nav = NavigationComputer::new();
    let nav = nav.plot_course().expect("plotting should start from Idle");
    let nav = nav
        .lock_course()
        .expect("course should lock once guards pass");
    // Type is NavigationComputer<CourseLocked>

    let diagnostic_error = nav
        .request_replot()
        .expect_err("diagnostics must pass before replotting");
    let (nav, guard_err) = diagnostic_error;
    assert_eq!(guard_err.guard, "diagnostics_passing");
    assert_eq!(guard_err.event, "request_replot");

    NavigationComputer::<Idle>::set_diagnostics_passing(true);
    let nav = nav
        .request_replot()
        .expect("replot should succeed when diagnostics pass");
    // Type is NavigationComputer<Replotting>

    let _nav = nav
        .plot_course()
        .expect("replotting should flow back to plotting");
}

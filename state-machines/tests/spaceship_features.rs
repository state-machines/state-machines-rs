use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use state_machines::{TransitionErrorKind, state_machine};

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

impl NavigationComputer {
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
    NavigationComputer::set_captain_on_bridge(false);
    NavigationComputer::set_solution_valid(false);

    let mut nav = NavigationComputer::new();
    nav.plot_course().expect("plotting should start from Idle");
    assert_eq!(nav.state(), NavState::Plotting);

    let auth_error = nav
        .lock_course()
        .expect_err("cannot lock course without the captain present");
    assert_eq!(
        auth_error.kind,
        TransitionErrorKind::GuardFailed {
            guard: "captain_on_bridge"
        }
    );
    assert_eq!(auth_error.from, NavState::Plotting);

    NavigationComputer::set_captain_on_bridge(true);
    let solution_error = nav
        .lock_course()
        .expect_err("solution must be valid before locking course");
    assert_eq!(
        solution_error.kind,
        TransitionErrorKind::GuardFailed {
            guard: "solution_valid"
        }
    );

    NavigationComputer::set_solution_valid(true);
    assert!(nav.can_lock_course());
    nav.lock_course()
        .expect("lock_course should succeed once all guards pass");
    assert_eq!(nav.state(), NavState::CourseLocked);
}

#[test]
fn replot_requires_transition_guard() {
    let _guard = NAV_TEST_LOCK.lock().unwrap();
    NavigationComputer::set_captain_on_bridge(true);
    NavigationComputer::set_solution_valid(true);
    NavigationComputer::set_diagnostics_passing(false);

    let mut nav = NavigationComputer::new();
    nav.plot_course().expect("plotting should start from Idle");
    nav.lock_course()
        .expect("course should lock once guards pass");
    assert!(!nav.can_request_replot());

    let diagnostic_error = nav
        .request_replot()
        .expect_err("diagnostics must pass before replotting");
    assert_eq!(
        diagnostic_error.kind,
        TransitionErrorKind::GuardFailed {
            guard: "diagnostics_passing"
        }
    );

    NavigationComputer::set_diagnostics_passing(true);
    assert!(nav.can_request_replot());
    nav.request_replot()
        .expect("replot should succeed when diagnostics pass");
    assert_eq!(nav.state(), NavState::Replotting);

    nav.plot_course()
        .expect("replotting should flow back to plotting");
    assert_eq!(nav.state(), NavState::Plotting);
}

#[test]
fn metadata_reflects_navigation_computer() {
    let _guard = NAV_TEST_LOCK.lock().unwrap();
    let definition = NavigationComputer::definition();
    assert_eq!(definition.name, "NavigationComputer");
    assert_eq!(definition.initial, NavState::Idle);
    assert_eq!(definition.events.len(), 4);

    let lock_course = &definition.events[1];
    assert_eq!(lock_course.name, "lock_course");
    assert_eq!(lock_course.guards, &["captain_on_bridge"]);
    assert!(lock_course.payload.is_none());
    assert_eq!(lock_course.transitions.len(), 1);
    let lock_transition = &lock_course.transitions[0];
    assert_eq!(lock_transition.sources, &[NavState::Plotting]);
    assert_eq!(lock_transition.target, NavState::CourseLocked);
    assert_eq!(lock_transition.guards, &["solution_valid"]);
    assert!(lock_transition.unless.is_empty());

    let plot_course = &definition.events[0];
    assert_eq!(plot_course.transitions.len(), 2);
    assert!(plot_course.payload.is_none());
    let sources_sets: Vec<&[NavState]> = plot_course
        .transitions
        .iter()
        .map(|transition| transition.sources)
        .collect();
    assert!(
        sources_sets
            .iter()
            .any(|sources| *sources == [NavState::Idle])
    );
    assert!(
        sources_sets
            .iter()
            .any(|sources| *sources == [NavState::Replotting])
    );
}

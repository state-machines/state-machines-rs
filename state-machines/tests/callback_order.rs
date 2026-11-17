//! Callbacks run in Ruby `state_machines`' order: guards first; before and
//! around callbacks in one declaration-ordered list, an around wrapping
//! everything declared after it; the state change; around exits innermost
//! first; then after callbacks in declaration order.
//!
//! The reference trace is Ruby's, from state_machines-diagram's
//! `Introspection.callback_sequence` test: with before1, around1, before2,
//! around2, after1 declared in that order the core runs before1, around1
//! enter, before2, around2 enter, the state change, around2 exit, around1
//! exit, after1.

use std::assert_matches;
use std::cell::RefCell;

use state_machines::{
    MachineState,
    core::{AroundOutcome, AroundStage},
    state_machine,
};

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: &'static str) {
    LOG.with_borrow_mut(|log| log.push(entry));
}

fn take_log() -> Vec<&'static str> {
    LOG.with_borrow_mut(std::mem::take)
}

/// Log `enter` or `exit` for an around stage and let the transition proceed.
fn wrap<S: MachineState>(
    stage: AroundStage,
    enter: &'static str,
    exit: &'static str,
) -> AroundOutcome<S> {
    log(match stage {
        AroundStage::Before => enter,
        AroundStage::AfterSuccess => exit,
    });
    AroundOutcome::Proceed
}

const RUBY_TRACE: [&str; 9] = [
    "before1",
    "around1 enter",
    "before2",
    "around2 enter",
    "exit hook",
    "enter hook",
    "around2 exit",
    "around1 exit",
    "after1",
];

state_machine! {
    name: Interleaved,
    initial: Idle,
    states: [Idle, Done],
    lifecycle: { Idle { exit: [left_idle] }, Done { enter: [entered_done] } },
    events {
        go {
            before: [before1],
            around: [around1],
            before: [before2],
            around: [around2],
            after: [after1],
            transition: { from: Idle, to: Done }
        }
    }
}

impl<C, S> Interleaved<C, S> {
    fn before1(&self) {
        log("before1");
    }
    fn before2(&self) {
        log("before2");
    }
    fn after1(&self) {
        log("after1");
    }
    fn left_idle(&self) {
        log("exit hook");
    }
    fn entered_done(&self) {
        log("enter hook");
    }
    fn around1(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        wrap(stage, "around1 enter", "around1 exit")
    }
    fn around2(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        wrap(stage, "around2 enter", "around2 exit")
    }
}

#[test]
fn interleaved_declarations_run_in_rubys_order() {
    take_log();
    Interleaved::new(()).go().expect("go");
    assert_eq!(take_log(), RUBY_TRACE);
}

state_machine! {
    name: Nested,
    initial: Parked,
    states: [Parked, Moved],
    lifecycle: { Moved { enter: [entered_moved] } },
    events {
        go {
            around: [event_outer, event_inner],
            after: [event_after],
            transition: {
                from: Parked,
                to: Moved,
                before: [transition_before],
                around: [transition_wrap],
                after: [transition_after],
            }
        }
    },
    callbacks: {
        around_transition [{ name: global_wrap }],
        before_transition [{ name: global_before }],
        after_transition [{ name: global_after }],
    }
}

impl<C, S> Nested<C, S> {
    fn global_before(&self) {
        log("global before");
    }
    fn global_after(&self) {
        log("global after");
    }
    fn transition_before(&self) {
        log("transition before");
    }
    fn event_after(&self) {
        log("event after");
    }
    fn transition_after(&self) {
        log("transition after");
    }
    fn entered_moved(&self) {
        log("entered Moved");
    }
    fn global_wrap(&self, stage: AroundStage) -> AroundOutcome<Parked> {
        wrap(stage, "global enter", "global exit")
    }
    fn event_outer(&self, stage: AroundStage) -> AroundOutcome<Parked> {
        wrap(stage, "event outer enter", "event outer exit")
    }
    fn event_inner(&self, stage: AroundStage) -> AroundOutcome<Parked> {
        wrap(stage, "event inner enter", "event inner exit")
    }
    fn transition_wrap(&self, stage: AroundStage) -> AroundOutcome<Parked> {
        wrap(stage, "transition enter", "transition exit")
    }
}

/// The `callbacks:` block counts as declared first, then the event, then the
/// transition; within each, declaration order. Arounds exit innermost first.
#[test]
fn nested_arounds_exit_innermost_first() {
    take_log();
    Nested::new(()).go().expect("go");
    assert_eq!(
        take_log(),
        [
            "global enter",
            "global before",
            "event outer enter",
            "event inner enter",
            "transition before",
            "transition enter",
            "entered Moved",
            "transition exit",
            "event inner exit",
            "event outer exit",
            "global exit",
            "global after",
            "event after",
            "transition after",
        ]
    );
}

state_machine! {
    name: Gate,
    initial: Closed,
    states: [Closed, Open, Jammed],
    events {
        open {
            guards: [unlocked],
            around: [open_wrap],
            before: [open_before],
            transition: { from: Closed, to: Open }
        }
        force {
            unless: [alarmed],
            around: [force_wrap],
            transition: { from: Closed, to: Open }
        }
        try_open {
            branching: true,
            transition: { from: Closed, to: Open, guards: [unlocked], around: [open_wrap] }
            transition: { from: Closed, to: Jammed, fallback: true, around: [jam_wrap] }
        }
    }
}

thread_local! {
    static UNLOCKED: RefCell<bool> = const { RefCell::new(false) };
}

impl<C, S> Gate<C, S> {
    fn unlocked(&self, _ctx: &C) -> bool {
        log("guard unlocked");
        UNLOCKED.with_borrow(|unlocked| *unlocked)
    }
    fn alarmed(&self, _ctx: &C) -> bool {
        log("guard alarmed");
        true
    }
    fn open_before(&self) {
        log("open before");
    }
    fn open_wrap(&self, stage: AroundStage) -> AroundOutcome<Closed> {
        wrap(stage, "open enter", "open exit")
    }
    fn force_wrap(&self, stage: AroundStage) -> AroundOutcome<Closed> {
        wrap(stage, "force enter", "force exit")
    }
    fn jam_wrap(&self, stage: AroundStage) -> AroundOutcome<Closed> {
        wrap(stage, "jam enter", "jam exit")
    }
}

#[test]
fn an_around_never_runs_when_its_guard_rejects() {
    take_log();
    UNLOCKED.set(false);
    let (gate, error) = Gate::new(()).open().expect_err("locked");
    assert_eq!(error.guard, "unlocked");
    assert_eq!(take_log(), ["guard unlocked"]);

    let (gate, error) = gate.force().expect_err("alarmed");
    assert_eq!(error.guard, "alarmed");
    assert_eq!(take_log(), ["guard alarmed"]);

    // Guards are checked before the around enters, not inside it
    UNLOCKED.set(true);
    gate.open().expect("unlocked");
    assert_eq!(
        take_log(),
        ["guard unlocked", "open enter", "open before", "open exit"]
    );
}

#[test]
fn only_the_selected_candidates_around_runs() {
    take_log();
    UNLOCKED.set(false);
    let outcome = Gate::new(()).try_open().expect("fallback");
    assert_matches!(outcome, GateClosedTryOpenOutcome::Jammed(_));
    assert_eq!(take_log(), ["guard unlocked", "jam enter", "jam exit"]);

    UNLOCKED.set(true);
    let outcome = Gate::new(()).try_open().expect("unlocked");
    assert_matches!(outcome, GateClosedTryOpenOutcome::Open(_));
    assert_eq!(take_log(), ["guard unlocked", "open enter", "open exit"]);
}

state_machine! {
    name: Wrapped,
    initial: Pending,
    states: [Pending, Settled],
    events {
        go {
            after: [after1],
            around: [outer],
            after: [after2],
            around: [inner],
            after: [after3],
            transition: { from: Pending, to: Settled }
        }
    }
}

impl<C, S> Wrapped<C, S> {
    fn after1(&self) {
        log("after1");
    }
    fn after2(&self) {
        log("after2");
    }
    fn after3(&self) {
        log("after3");
    }
    fn outer(&self, stage: AroundStage) -> AroundOutcome<Pending> {
        wrap(stage, "outer enter", "outer exit")
    }
    fn inner(&self, stage: AroundStage) -> AroundOutcome<Pending> {
        wrap(stage, "inner enter", "inner exit")
    }
}

/// Ruby runs after callbacks once every around has exited, whatever their
/// declaration order relative to the arounds.
#[test]
fn after_callbacks_run_once_every_around_has_exited() {
    take_log();
    Wrapped::new(()).go().expect("go");
    assert_eq!(
        take_log(),
        [
            "outer enter",
            "inner enter",
            "inner exit",
            "outer exit",
            "after1",
            "after2",
            "after3",
        ]
    );
}

state_machine! {
    name: Fallible,
    initial: Ready,
    states: [Ready, Spent],
    error: &'static str,
    events {
        go {
            around: [outer],
            before: [failing_before],
            after: [after1],
            on_error: [failed],
            transition: { from: Ready, to: Spent }
        }
        finish {
            around: [failing_exit],
            after: [after1],
            on_error: [failed],
            transition: { from: Ready, to: Spent }
        }
    }
}

impl<C, S> Fallible<C, S> {
    fn failing_before(&self) -> Result<(), &'static str> {
        log("failing before");
        Err("nope")
    }
    fn after1(&self) {
        log("after1");
    }
    fn failed(&self, _error: &state_machines::EventError<&'static str>) {
        log("on_error");
    }
    fn outer(&self, stage: AroundStage) -> AroundOutcome<Ready> {
        wrap(stage, "outer enter", "outer exit")
    }
    fn failing_exit(&self, stage: AroundStage) -> AroundOutcome<Ready> {
        match stage {
            AroundStage::Before => wrap(stage, "exit enter", ""),
            AroundStage::AfterSuccess => {
                log("exit aborts");
                AroundOutcome::Abort(state_machines::core::TransitionError::guard_failed(
                    Ready,
                    "finish",
                    "failing_exit",
                ))
            }
        }
    }
}

/// A failure stops the run where it happens: an around entered before it
/// never exits (as in Ruby, a halt unwinds through the around's yield), and
/// no after callback runs.
#[test]
fn a_failure_inside_an_around_skips_its_exit_and_the_afters() {
    take_log();
    let (machine, error) = Fallible::new(()).go().expect_err("before fails");
    assert_matches!(error, state_machines::EventError::Callback(_));
    assert_eq!(take_log(), ["outer enter", "failing before", "on_error"]);

    let (_machine, _error) = machine.finish().expect_err("around exit aborts");
    assert_eq!(take_log(), ["exit enter", "exit aborts", "on_error"]);
}

state_machine! {
    name: Dispatched,
    dynamic: true,
    initial: Queued,
    states: [Queued, Sent],
    lifecycle: { Queued { exit: [left_queued] }, Sent { enter: [entered_sent] } },
    events {
        go {
            before: [before1],
            around: [around1],
            before: [before2],
            around: [around2],
            after: [after1],
            transition: { from: Queued, to: Sent }
        }
    }
}

impl<C, S> Dispatched<C, S> {
    fn before1(&self) {
        log("before1");
    }
    fn before2(&self) {
        log("before2");
    }
    fn after1(&self) {
        log("after1");
    }
    fn left_queued(&self) {
        log("exit hook");
    }
    fn entered_sent(&self) {
        log("enter hook");
    }
    fn around1(&self, stage: AroundStage) -> AroundOutcome<Queued> {
        wrap(stage, "around1 enter", "around1 exit")
    }
    fn around2(&self, stage: AroundStage) -> AroundOutcome<Queued> {
        wrap(stage, "around2 enter", "around2 exit")
    }
}

#[test]
fn dynamic_dispatch_runs_the_same_order() {
    take_log();
    let mut machine = DynamicDispatched::new(());
    machine.handle(DispatchedEvent::Go).expect("go dispatches");
    assert_eq!(take_log(), RUBY_TRACE);
}

#[cfg(feature = "async")]
#[test]
fn async_transitions_run_the_same_order() {
    use std::future::{Future, ready};

    state_machine! {
        name: AsyncInterleaved,
        async: true,
        initial: Idle,
        states: [Idle, Done],
        lifecycle: { Idle { exit: [left_idle] }, Done { enter: [entered_done] } },
        events {
            go {
                before: [before1],
                around: [around1],
                before: [before2],
                around: [around2],
                after: [after1],
                transition: { from: Idle, to: Done }
            }
        }
    }

    fn logged(entry: &'static str) -> impl Future<Output = ()> {
        log(entry);
        ready(())
    }

    impl<C, S> AsyncInterleaved<C, S> {
        fn before1(&self) -> impl Future<Output = ()> {
            logged("before1")
        }
        fn before2(&self) -> impl Future<Output = ()> {
            logged("before2")
        }
        fn after1(&self) -> impl Future<Output = ()> {
            logged("after1")
        }
        fn left_idle(&self) -> impl Future<Output = ()> {
            logged("exit hook")
        }
        fn entered_done(&self) -> impl Future<Output = ()> {
            logged("enter hook")
        }
        fn around1(&self, stage: AroundStage) -> impl Future<Output = AroundOutcome<Idle>> {
            ready(wrap(stage, "around1 enter", "around1 exit"))
        }
        fn around2(&self, stage: AroundStage) -> impl Future<Output = AroundOutcome<Idle>> {
            ready(wrap(stage, "around2 enter", "around2 exit"))
        }
    }

    take_log();
    pollster::block_on(AsyncInterleaved::new(()).go()).expect("go");
    assert_eq!(take_log(), RUBY_TRACE);
}

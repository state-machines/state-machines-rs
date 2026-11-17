//! The state change is the commit point, as in Ruby `state_machines`.
//!
//! Before it (guards, before callbacks, around `Before` stages, exit hooks) a
//! failure means no transition: the caller gets the source-state machine
//! back. After it the machine is in the target state no matter what: an
//! around halting after its yield skips the remaining around exits and after
//! callbacks, and a failing fallible callback is surfaced together with the
//! target-state machine.

use state_machines::{
    DynamicError, EventError, TransitionFailure,
    core::{AroundOutcome, AroundStage, TransitionError},
    state_machine,
};
use std::assert_matches;
use std::cell::RefCell;

thread_local! {
    static LOG: RefCell<Vec<&'static str>> = const { RefCell::new(Vec::new()) };
}

fn log(entry: &'static str) {
    LOG.with(|log| log.borrow_mut().push(entry));
}

fn take_log() -> Vec<&'static str> {
    LOG.with(|log| std::mem::take(&mut *log.borrow_mut()))
}

/// Which callback fails or halts in one run.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Fault {
    #[default]
    None,
    AbortBefore,
    AbortAfter,
    FailBefore,
    FailEnter,
    FailAfter,
}

/// The test context: the fault to inject, and which branch `route` takes.
#[derive(Debug, Default)]
pub struct Flags {
    fault: Fault,
    go_right: bool,
}

const fn with(fault: Fault) -> Flags {
    Flags {
        fault,
        go_right: false,
    }
}

// ---------------------------------------------------------------------------
// Infallible machine: an around halting after its yield keeps the transition.
// ---------------------------------------------------------------------------

state_machine! {
    name: Halting,
    dynamic: true,
    context: Flags,
    initial: Idle,
    states: [Idle, Busy],
    final_states: [Busy],
    lifecycle: { Busy { enter: [entered_busy], complete: [busy_complete] } },
    events {
        go {
            around: [outer, inner],
            after: [after1, after2],
            transition: { from: Idle, to: Busy }
        }
    }
}

impl<S> Halting<S> {
    fn entered_busy(&self) {
        log("enter busy");
    }
    fn busy_complete(&self) {
        log("complete busy");
    }
    fn after1(&self) {
        log("after1");
    }
    fn after2(&self) {
        log("after2");
    }
    fn outer(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        match stage {
            AroundStage::Before => log("outer enter"),
            AroundStage::AfterSuccess => log("outer exit"),
        }
        AroundOutcome::Proceed
    }
    fn inner(&self, stage: AroundStage) -> AroundOutcome<Idle> {
        let halt = AroundOutcome::Abort(TransitionError::guard_failed(Idle, "go", "inner"));
        match stage {
            AroundStage::Before if self.ctx.fault == Fault::AbortBefore => {
                log("inner rejects");
                halt
            }
            AroundStage::Before => {
                log("inner enter");
                AroundOutcome::Proceed
            }
            AroundStage::AfterSuccess if self.ctx.fault == Fault::AbortAfter => {
                log("inner halts");
                halt
            }
            AroundStage::AfterSuccess => {
                log("inner exit");
                AroundOutcome::Proceed
            }
        }
    }
}

const HALTED_TRACE: [&str; 5] = [
    "outer enter",
    "inner enter",
    "enter busy",
    "inner halts",
    // outer exit, after1 and after2 are skipped; the state's own completion
    // hook still runs, because the final state was reached.
    "complete busy",
];

#[test]
fn around_halting_after_yield_keeps_target_and_skips_the_rest() {
    take_log();
    let machine = Halting::new(with(Fault::AbortAfter));
    let busy: Halting<Busy> = machine.go().expect("the state change stands");
    assert!(busy.is_finished());
    assert_eq!(take_log(), HALTED_TRACE);
}

#[test]
fn around_proceeding_runs_every_exit_and_after() {
    take_log();
    let _busy: Halting<Busy> = Halting::new(with(Fault::None)).go().expect("go");
    assert_eq!(
        take_log(),
        [
            "outer enter",
            "inner enter",
            "enter busy",
            "inner exit",
            "outer exit",
            "after1",
            "after2",
            "complete busy",
        ]
    );
}

#[test]
fn around_rejecting_before_yield_returns_the_source_machine() {
    take_log();
    let machine = Halting::new(with(Fault::AbortBefore));
    let (_, error): (Halting<Idle>, _) = machine.go().expect_err("rejected");
    assert_eq!(error.guard, "inner");
    assert_eq!(take_log(), ["outer enter", "inner rejects"]);
}

#[test]
fn dynamic_around_halting_after_yield_commits() {
    take_log();
    let mut machine = DynamicHalting::new(with(Fault::AbortAfter));
    machine
        .handle(HaltingEvent::Go)
        .expect("halt after yield is not a failure");
    assert_eq!(machine.current_state(), HaltingState::Busy);
    assert_eq!(machine.transition_epoch(), 1);
    assert_eq!(
        machine.take_completion_events(),
        [state_machines::CompletionEvent::Machine]
    );
    assert_eq!(take_log(), HALTED_TRACE);
}

#[test]
fn dynamic_around_rejecting_before_yield_keeps_source() {
    take_log();
    let mut machine = DynamicHalting::new(with(Fault::AbortBefore));
    let error = machine.handle(HaltingEvent::Go).expect_err("rejected");
    assert_matches!(error, DynamicError::GuardFailed { guard: "inner", .. });
    assert!(!error.is_committed());
    assert_eq!(machine.current_state(), HaltingState::Idle);
    assert_eq!(machine.transition_epoch(), 0);
}

// ---------------------------------------------------------------------------
// Fallible machine: a failing callback after the state change is surfaced
// with the target-state machine.
// ---------------------------------------------------------------------------

state_machine! {
    name: Ledger,
    dynamic: true,
    context: Flags,
    error: &'static str,
    initial: Open,
    states: [Open, Posted, Closed],
    lifecycle: { Open { exit: [left_open] }, Posted { enter: [entered_posted] } },
    events {
        post {
            before: [check],
            after: [first_after, failing_after, last_after],
            on_error: [failed],
            transition: { from: Open, to: Posted }
        }
        close {
            transition: { from: Posted, to: Closed }
        }
    }
}

impl<S> Ledger<S> {
    fn check(&self) -> Result<(), &'static str> {
        log("check");
        if self.ctx.fault == Fault::FailBefore {
            Err("before")
        } else {
            Ok(())
        }
    }
    fn left_open(&self) {
        log("exit open");
    }
    fn entered_posted(&self) -> Result<(), &'static str> {
        log("enter posted");
        if self.ctx.fault == Fault::FailEnter {
            Err("enter")
        } else {
            Ok(())
        }
    }
    fn first_after(&self) {
        log("first after");
    }
    fn failing_after(&self) -> Result<(), &'static str> {
        log("failing after");
        if self.ctx.fault == Fault::FailAfter {
            Err("after")
        } else {
            Ok(())
        }
    }
    fn last_after(&self) {
        log("last after");
    }
    fn failed(&self, _error: &EventError<&'static str>) {
        log("on_error");
    }
}

#[test]
fn failing_after_keeps_target_and_surfaces_the_error() {
    take_log();
    let machine = Ledger::new(with(Fault::FailAfter));
    let failure = machine.post().expect_err("after fails");
    assert!(failure.is_committed());
    let TransitionFailure::Committed { machine, error } = failure else {
        panic!("a failing after callback must not revert the state change");
    };
    // The committed machine is typed as the target state: `close` only
    // exists on `Ledger<Posted>`.
    let posted: Ledger<Posted> = machine;
    assert_eq!(error.action, "failing_after");
    assert_eq!(error.event, "post");
    assert_eq!(error.source, "after");
    assert_eq!(
        take_log(),
        [
            "check",
            "exit open",
            "enter posted",
            "first after",
            "failing after",
            // last after is skipped; on_error does not run, nothing was
            // rejected.
        ]
    );
    let _closed: Ledger<Closed> = posted.close().expect("posted machine is live");
}

#[test]
fn failing_entry_hook_keeps_target_and_skips_afters() {
    take_log();
    let failure = Ledger::new(with(Fault::FailEnter))
        .post()
        .expect_err("entry hook fails");
    let TransitionFailure::Committed { machine, error } = failure else {
        panic!("entry hooks run after the state change");
    };
    let _: Ledger<Posted> = machine;
    assert_eq!(error.action, "entered_posted");
    assert_eq!(take_log(), ["check", "exit open", "enter posted"]);
}

#[test]
fn failing_before_returns_the_source_machine() {
    take_log();
    let failure = Ledger::new(with(Fault::FailBefore))
        .post()
        .expect_err("before fails");
    assert!(!failure.is_committed());
    let TransitionFailure::Rejected { machine, error } = failure else {
        panic!("a before callback runs ahead of the state change");
    };
    let _: Ledger<Open> = machine;
    assert_matches!(
        error,
        EventError::Callback(ref callback) if callback.action == "check"
    );
    assert_eq!(take_log(), ["check", "on_error"]);
}

#[test]
fn transition_failure_displays_which_side_of_the_commit_it_is() {
    let committed = Ledger::new(with(Fault::FailAfter))
        .post()
        .expect_err("after fails");
    assert_eq!(
        committed.to_string(),
        "transition committed, then callback `failing_after` failed for `post`: \"after\""
    );
    let rejected = Ledger::new(with(Fault::FailBefore))
        .post()
        .expect_err("before fails");
    assert_eq!(
        rejected.to_string(),
        "callback `check` failed for `post`: \"before\""
    );
    take_log();
}

#[test]
fn dynamic_failing_after_commits_and_reports_it() {
    take_log();
    let mut machine = DynamicLedger::new(with(Fault::FailAfter));
    let error = machine.handle(LedgerEvent::Post).expect_err("after fails");
    assert!(error.is_committed());
    assert_matches!(
        error,
        DynamicError::CommittedCallbackFailed {
            action: "failing_after",
            event: "post",
            source: "after",
        }
    );
    assert_eq!(machine.current_state(), LedgerState::Posted);
    assert_eq!(machine.transition_epoch(), 1);
    assert!(!machine.is_poisoned());
    machine
        .handle(LedgerEvent::Close)
        .expect("machine moved on");
    assert_eq!(machine.current_state(), LedgerState::Closed);
}

#[test]
fn dynamic_failing_before_keeps_source() {
    take_log();
    let mut machine = DynamicLedger::new(with(Fault::FailBefore));
    let error = machine.handle(LedgerEvent::Post).expect_err("before fails");
    assert!(!error.is_committed());
    assert_matches!(
        error,
        DynamicError::CallbackFailed {
            action: "check",
            ..
        }
    );
    assert_eq!(machine.current_state(), LedgerState::Open);
    assert_eq!(machine.transition_epoch(), 0);
}

// ---------------------------------------------------------------------------
// Branching, automatic transitions and history follow the same rule.
// ---------------------------------------------------------------------------

state_machine! {
    name: Router,
    dynamic: true,
    context: Flags,
    error: &'static str,
    initial: Start,
    states: [Start, Left, Right, Done],
    final_states: [Done],
    events {
        route {
            branching: true,
            after: [failing_after],
            transition: { from: Start, to: Left, guards: [go_left] }
            transition: { from: Start, to: Right, fallback: true }
        }
        settle {
            automatic: true,
            after: [failing_after],
            transition: { from: Left, to: Done }
        }
    }
}

impl<S> Router<S> {
    const fn go_left(&self, ctx: &Flags) -> bool {
        !ctx.go_right
    }
    fn failing_after(&self) -> Result<(), &'static str> {
        if self.ctx.fault == Fault::FailAfter {
            Err("after")
        } else {
            Ok(())
        }
    }
}

#[test]
fn branching_failure_after_commit_carries_the_outcome() {
    let failure = Router::new(Flags {
        fault: Fault::FailAfter,
        go_right: true,
    })
    .route()
    .expect_err("after fails");
    let TransitionFailure::Committed { machine, error } = failure else {
        panic!("branch committed before its after failed");
    };
    assert_matches!(machine, RouterStartRouteOutcome::Right(_));
    assert_eq!(error.action, "failing_after");
}

#[test]
fn automatic_failure_after_commit_keeps_target() {
    let mut machine = DynamicRouter::new(with(Fault::FailAfter));
    // `route` commits to Left, then its after fails: settling never starts.
    let error = machine.handle(RouterEvent::Route).expect_err("after fails");
    assert!(error.is_committed());
    assert_eq!(machine.current_state(), RouterState::Left);
    // Stabilizing runs `settle`, which commits to Done before its after fails.
    let error = machine.stabilize(4).expect_err("automatic after fails");
    assert_matches!(
        error,
        DynamicError::CommittedCallbackFailed {
            action: "failing_after",
            event: "settle",
            ..
        }
    );
    assert_eq!(machine.current_state(), RouterState::Done);
    assert_eq!(
        machine.take_completion_events(),
        [state_machines::CompletionEvent::Machine]
    );
}

state_machine! {
    name: Remembering,
    context: Flags,
    error: &'static str,
    initial: First,
    states: [superstate Active { state First, state Second }, Off],
    events {
        advance { transition: { from: First, to: Second } }
        shut {
            after: [failing_after],
            transition: { from: Active, to: Off }
        }
        resume { transition: { from: Off, to: Active, history: deep } }
    }
}

impl<S> Remembering<S> {
    fn failing_after(&self) -> Result<(), &'static str> {
        if self.ctx.fault == Fault::FailAfter {
            Err("after")
        } else {
            Ok(())
        }
    }
}

#[test]
fn history_is_recorded_even_when_an_after_fails() {
    let second = Remembering::new(with(Fault::FailAfter))
        .advance()
        .expect("advance");
    let TransitionFailure::Committed { machine: off, .. } = second.shut().expect_err("after fails")
    else {
        panic!("shut committed before its after failed");
    };
    let resumed = off.resume().expect("resume");
    assert_matches!(resumed, RememberingOffResumeOutcome::Second(_));
}

// ---------------------------------------------------------------------------
// The runtime sees the committed state too.
// ---------------------------------------------------------------------------

#[cfg(feature = "runtime")]
#[test]
fn runner_reports_a_committed_failure_with_the_target_state() {
    use state_machines::runtime::{Machine, RunError, Runner};

    let mut runner = Runner::new(DynamicLedger::new(with(Fault::FailAfter)), 4);
    runner.enqueue(LedgerEvent::Post).unwrap();
    runner.enqueue(LedgerEvent::Close).unwrap();
    let error = pollster::block_on(runner.drain(4)).expect_err("after fails");
    assert_matches!(
        error,
        RunError::Dispatch(DynamicError::CommittedCallbackFailed {
            action: "failing_after",
            ..
        })
    );
    assert_eq!(runner.machine().state(), LedgerState::Posted);
    assert_eq!(pollster::block_on(runner.drain(4)), Ok(1));
    assert_eq!(runner.machine().state(), LedgerState::Closed);
    take_log();
}

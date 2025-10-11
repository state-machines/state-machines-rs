use criterion::{Criterion, black_box, criterion_group, criterion_main};
use state_machines::{
    core::{AroundOutcome, AroundStage},
    state_machine,
};

// ============================================================================
// Baseline: Simple state machine with no guards/callbacks
// ============================================================================

state_machine! {
    name: SimpleLight,
    initial: Off,
    states: [Off, On],
    events {
        toggle_on {
            transition: { from: Off, to: On }
        }
        toggle_off {
            transition: { from: On, to: Off }
        }
    }
}

fn benchmark_simple_transition(c: &mut Criterion) {
    c.bench_function("simple_transition", |b| {
        b.iter(|| {
            let light = SimpleLight::new(());
            let light = black_box(light.toggle_on().unwrap());
            let light = black_box(light.toggle_off().unwrap());
            black_box(light)
        });
    });
}

// ============================================================================
// With Guards: Test guard evaluation overhead
// ============================================================================

state_machine! {
    name: GuardedDoor,
    initial: Closed,
    states: [Closed, Open],
    events {
        open {
            guards: [check_permission],
            transition: { from: Closed, to: Open }
        }
        close {
            transition: { from: Open, to: Closed }
        }
    }
}

impl<C, S> GuardedDoor<C, S> {
    fn check_permission(&self, _ctx: &C) -> bool {
        true // Always pass for benchmark
    }
}

fn benchmark_guarded_transition(c: &mut Criterion) {
    c.bench_function("guarded_transition", |b| {
        b.iter(|| {
            let door = GuardedDoor::new(());
            let door = black_box(door.open().unwrap());
            let door = black_box(door.close().unwrap());
            black_box(door)
        });
    });
}

// ============================================================================
// With Callbacks: Test callback overhead
// ============================================================================

state_machine! {
    name: CallbackEngine,
    initial: EngineStopped,
    states: [EngineStopped, EngineRunning],
    events {
        start {
            before: [pre_start_check],
            after: [log_started],
            transition: { from: EngineStopped, to: EngineRunning }
        }
        stop {
            transition: { from: EngineRunning, to: EngineStopped }
        }
    }
}

impl<C, S> CallbackEngine<C, S> {
    fn pre_start_check(&self) {
        // Minimal work
    }

    fn log_started(&self) {
        // Minimal work
    }
}

fn benchmark_callback_transition(c: &mut Criterion) {
    c.bench_function("callback_transition", |b| {
        b.iter(|| {
            let engine = CallbackEngine::new(());
            let engine = black_box(engine.start().unwrap());
            let engine = black_box(engine.stop().unwrap());
            black_box(engine)
        });
    });
}

// ============================================================================
// Guards + Callbacks: Combined overhead
// ============================================================================

state_machine! {
    name: FullFeatured,
    initial: FullIdle,
    states: [FullIdle, FullActive],
    events {
        activate {
            guards: [can_activate],
            before: [prepare],
            after: [notify],
            transition: { from: FullIdle, to: FullActive }
        }
        deactivate {
            transition: { from: FullActive, to: FullIdle }
        }
    }
}

impl<C, S> FullFeatured<C, S> {
    fn can_activate(&self, _ctx: &C) -> bool {
        true
    }

    fn prepare(&self) {
        // Minimal work
    }

    fn notify(&self) {
        // Minimal work
    }
}

fn benchmark_full_featured_transition(c: &mut Criterion) {
    c.bench_function("full_featured_transition", |b| {
        b.iter(|| {
            let machine = FullFeatured::new(());
            let machine = black_box(machine.activate().unwrap());
            let machine = black_box(machine.deactivate().unwrap());
            black_box(machine)
        });
    });
}

// ============================================================================
// Hierarchical States: Test superstate transition overhead
// ============================================================================

#[derive(Default, Debug, Clone)]
struct SubData {
    value: u32,
}

state_machine! {
    name: HierarchicalMachine,
    initial: HierStandby,
    states: [
        HierStandby,
        superstate HierActive {
            state SubA(SubData),
            state SubB(SubData),
        },
        HierStopped,
    ],
    events {
        enter_active {
            transition: { from: HierStandby, to: HierActive }
        }
        switch {
            transition: { from: SubA, to: SubB }
        }
        abort {
            transition: { from: HierActive, to: HierStandby }
        }
    }
}

fn benchmark_hierarchical_transition(c: &mut Criterion) {
    c.bench_function("hierarchical_transition", |b| {
        b.iter(|| {
            let machine = HierarchicalMachine::new(());
            let machine = black_box(machine.enter_active().unwrap());
            let machine = black_box(machine.switch().unwrap());
            let machine = black_box(machine.abort().unwrap());
            black_box(machine)
        });
    });
}

fn benchmark_hierarchical_polymorphic(c: &mut Criterion) {
    c.bench_function("hierarchical_polymorphic", |b| {
        b.iter(|| {
            let machine = HierarchicalMachine::new(());
            let machine = black_box(machine.enter_active().unwrap());
            // Polymorphic abort from SubA
            let machine = black_box(machine.abort().unwrap());
            black_box(machine)
        });
    });
}

// ============================================================================
// State Data Access: Test accessor overhead
// ============================================================================

fn benchmark_state_data_access(c: &mut Criterion) {
    c.bench_function("state_data_access", |b| {
        let machine = HierarchicalMachine::new(());
        let machine = machine.enter_active().unwrap();

        b.iter(|| {
            let data = black_box(machine.sub_a_data());
            black_box(data.value)
        });
    });
}

fn benchmark_state_data_mutation(c: &mut Criterion) {
    c.bench_function("state_data_mutation", |b| {
        b.iter(|| {
            let machine = HierarchicalMachine::new(());
            let mut machine = machine.enter_active().unwrap();
            machine.sub_a_data_mut().value = black_box(42);
            black_box(machine)
        });
    });
}

// ============================================================================
// Around Callbacks: Test around callback overhead
// ============================================================================

state_machine! {
    name: AroundTransaction,
    initial: AroundIdle,
    states: [AroundIdle, AroundProcessing, AroundComplete],
    events {
        begin {
            around: [transaction_wrapper],
            transition: { from: AroundIdle, to: AroundProcessing }
        }
        finish {
            transition: { from: AroundProcessing, to: AroundComplete }
        }
    }
}

impl<C, S> AroundTransaction<C, S> {
    fn transaction_wrapper(&self, stage: AroundStage) -> AroundOutcome<AroundIdle> {
        match stage {
            AroundStage::Before => AroundOutcome::Proceed,
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }
}

fn benchmark_around_transition(c: &mut Criterion) {
    c.bench_function("around_transition", |b| {
        b.iter(|| {
            let transaction = AroundTransaction::new(());
            let transaction = black_box(transaction.begin().unwrap());
            let transaction = black_box(transaction.finish().unwrap());
            black_box(transaction)
        });
    });
}

// ============================================================================
// Multiple Around Callbacks: Test multiple wrapper overhead
// ============================================================================

state_machine! {
    name: MultiAround,
    initial: MultiStart,
    states: [MultiStart, MultiEnd],
    events {
        advance {
            around: [first_wrapper, second_wrapper, third_wrapper],
            transition: { from: MultiStart, to: MultiEnd }
        }
    }
}

impl<C, S> MultiAround<C, S> {
    fn first_wrapper(&self, stage: AroundStage) -> AroundOutcome<MultiStart> {
        match stage {
            AroundStage::Before => AroundOutcome::Proceed,
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }

    fn second_wrapper(&self, stage: AroundStage) -> AroundOutcome<MultiStart> {
        match stage {
            AroundStage::Before => AroundOutcome::Proceed,
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }

    fn third_wrapper(&self, stage: AroundStage) -> AroundOutcome<MultiStart> {
        match stage {
            AroundStage::Before => AroundOutcome::Proceed,
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }
}

fn benchmark_multi_around_transition(c: &mut Criterion) {
    c.bench_function("multi_around_transition", |b| {
        b.iter(|| {
            let machine = MultiAround::new(());
            let machine = black_box(machine.advance().unwrap());
            black_box(machine)
        });
    });
}

// ============================================================================
// All Features Combined: Around + Guards + Callbacks
// ============================================================================

state_machine! {
    name: FullStack,
    initial: StackIdle,
    states: [StackIdle, StackActive],
    events {
        activate {
            guards: [can_proceed],
            around: [wrapper],
            before: [prepare],
            after: [cleanup],
            transition: { from: StackIdle, to: StackActive }
        }
        deactivate {
            transition: { from: StackActive, to: StackIdle }
        }
    }
}

impl<C, S> FullStack<C, S> {
    fn can_proceed(&self, _ctx: &C) -> bool {
        true
    }

    fn wrapper(&self, stage: AroundStage) -> AroundOutcome<StackIdle> {
        match stage {
            AroundStage::Before => AroundOutcome::Proceed,
            AroundStage::AfterSuccess => AroundOutcome::Proceed,
        }
    }

    fn prepare(&self) {
        // Minimal work
    }

    fn cleanup(&self) {
        // Minimal work
    }
}

fn benchmark_full_stack_transition(c: &mut Criterion) {
    c.bench_function("full_stack_transition", |b| {
        b.iter(|| {
            let machine = FullStack::new(());
            let machine = black_box(machine.activate().unwrap());
            let machine = black_box(machine.deactivate().unwrap());
            black_box(machine)
        });
    });
}

criterion_group!(
    benches,
    benchmark_simple_transition,
    benchmark_guarded_transition,
    benchmark_callback_transition,
    benchmark_full_featured_transition,
    benchmark_hierarchical_transition,
    benchmark_hierarchical_polymorphic,
    benchmark_state_data_access,
    benchmark_state_data_mutation,
    benchmark_around_transition,
    benchmark_multi_around_transition,
    benchmark_full_stack_transition,
);
criterion_main!(benches);

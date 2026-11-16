use super::{Kind, MAX_STATES, Request, State, on_chart_stack};

/// A chart past 4096 states resolves at `MAX_STATES` capacity on its own
/// thread, whatever stack the caller runs on.
#[test]
fn a_large_chart_resolves_on_a_small_caller_stack() {
    let states: Vec<State> = std::iter::once(State::compound(None, 1))
        .chain((1..5_000).map(|_| State::atomic(Some(0))))
        .collect();
    let requests = [Some(Request {
        leaf: 1,
        source: 1,
        target: 4_999,
        kind: Kind::External,
    })];
    let resolved = std::thread::Builder::new()
        .stack_size(128 * 1024)
        .spawn(move || on_chart_stack::<MAX_STATES>(&states, &requests))
        .expect("spawn")
        .join()
        .expect("join");
    assert_eq!(resolved, Ok(vec![(vec![1], vec![4_999])]));
}

//! Owned child invocation is a scoped activity, not a detached executor task.
//! Local mode changes retain it; external reset drops it and closes its channel.
use state_machines::{
    runtime::{EventSink, QueueError, Runner, run_child},
    state_machine,
};
use std::{
    assert_matches,
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

#[derive(Debug)]
struct Report(Vec<u8>);
#[derive(Debug)]
pub struct Packet(RefCell<Option<Vec<u8>>>);
#[derive(Debug)]
pub struct Delivery(Option<Report>);
#[derive(Debug, Default)]
pub struct TransferContext {
    receipt: RefCell<Option<Report>>,
}
state_machine! {
    name: PacketTransfer, context: TransferContext, dynamic: true, initial: Staged,
    states: [Staged, Sent], final_states: [Sent],
    events {
        send { payload: Packet, before: [receive], transition: { from: Staged, to: Sent } }
    }
}
impl<S> PacketTransfer<S> {
    fn receive(&self, packet: &Packet) {
        self.ctx.receipt.replace(Some(Report(
            packet
                .0
                .borrow_mut()
                .take()
                .expect("one owned transmission"),
        )));
    }
}

#[derive(Debug, Default)]
pub struct MissionContext {
    channel: Rc<RefCell<Option<EventSink<PacketTransferEvent>>>>,
    drops: Rc<Cell<usize>>,
    invocations: Rc<Cell<usize>>,
}
state_machine! {
    name: ScoutMission, context: MissionContext, dynamic: true, async: true, initial: Standby,
    states: [Standby, superstate Flying { state Acquiring, state Sending }, Complete(Report), Fault],
    final_states: [Complete],
    runtime: { Flying { invoke: [transfer] } },
    events {
        begin { transition: { from: Standby, to: Flying } }
        switch_mode { transition: { from: Flying, to: Sending, kind: local } }
        reset { transition: { from: Flying, to: Flying, kind: external } }
        delivered { payload: Delivery, transition: { from: Flying, to: Complete, data: take_report } }
        failed { payload: String, transition: { from: Flying, to: Fault } }
    }
}
struct Lease(Rc<Cell<usize>>);
impl Drop for Lease {
    fn drop(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}
impl<S> ScoutMission<S> {
    fn transfer(&self) -> impl Future<Output = ScoutMissionEvent> + 'static {
        self.ctx.invocations.set(self.ctx.invocations.get() + 1);
        let child = Runner::new(DynamicPacketTransfer::new(TransferContext::default()), 4);
        self.ctx.channel.replace(Some(child.sink()));
        let lease = Lease(self.ctx.drops.clone());
        async move {
            let _lease = lease;
            // The shared child driver waits using real mailbox wakers and yields
            // at batch limits. No polling loop or executor-specific join handle.
            match run_child(child, 4).await {
                Ok(child) => {
                    let report = child.into_sent().unwrap().ctx.receipt.into_inner().unwrap();
                    ScoutMissionEvent::Delivered(Delivery(Some(report)))
                }
                Err((error, _child)) => ScoutMissionEvent::Failed(format!("{error:?}")),
            }
        }
    }
    async fn take_report(&self, delivery: &mut Delivery) -> Report {
        delivery.0.take().expect("move report into the final state")
    }
}

pub async fn run_demo() {
    println!("\n=== Composite activity lifetime and invoked child ===");
    let context = MissionContext::default();
    let channel = context.channel.clone();
    let drops = context.drops.clone();
    let invocations = context.invocations.clone();
    let mut runner = Runner::new(DynamicScoutMission::new(context), 8);
    runner.enqueue(ScoutMissionEvent::Begin).unwrap();
    runner.drain(8).await.unwrap();
    let old_channel = channel.borrow().as_ref().unwrap().clone();
    runner.enqueue(ScoutMissionEvent::SwitchMode).unwrap();
    runner.drain(8).await.unwrap();
    assert_eq!(runner.machine().current_state(), ScoutMissionState::Sending);
    assert_eq!(
        invocations.get(),
        1,
        "local transition retains the activity"
    );
    assert_eq!(drops.get(), 0);

    // Reset this same mission, rather than constructing duplicate fixtures.
    runner.enqueue(ScoutMissionEvent::Reset).unwrap();
    runner.drain(8).await.unwrap();
    assert_eq!(drops.get(), 1, "reset dropped the old owned child future");
    assert_eq!(invocations.get(), 2);
    let error = old_channel
        .enqueue(PacketTransferEvent::Send(Packet(RefCell::new(Some(vec![
            10, 20, 30,
        ])))))
        .unwrap_err();
    assert_matches!(&error, QueueError::Closed(_));
    // Channel rejection returned the original owned packet; retry it on the
    // fresh child without cloning its bytes.
    let new_channel = channel.borrow().as_ref().unwrap().clone();
    new_channel.enqueue(error.into_event()).unwrap();
    runner.drain(8).await.unwrap();
    assert!(runner.machine().is_finished());
    assert_eq!(drops.get(), 2);
    let machine = runner.into_machine().into_complete().unwrap();
    assert_eq!(machine.complete_data().unwrap().0, [10, 20, 30]);
    println!("Reset closed the old channel; retried packet arrived once as an owned report.");
}

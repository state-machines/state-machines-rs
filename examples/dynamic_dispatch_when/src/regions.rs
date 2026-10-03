//! Compose the SAME computer from the basic hybrid example, rather than
//! introducing duplicate region machines or manually rebuilding snapshots.
use super::{
    Command, CommandSource, DynamicSpacecraftComputer, SpacecraftComputerEvent,
    SpacecraftComputerState,
};
use state_machines::{
    SnapshotError,
    runtime::{Machine, ParallelError},
    state_machine,
};
use std::assert_matches;

state_machine! {
    name: MissionConsole, snapshot: true,
    regions: { primary: DynamicSpacecraftComputer<()>, backup: DynamicSpacecraftComputer<()> },
    events {
        boot { routes: { primary: SpacecraftComputerEvent::Boot, backup: SpacecraftComputerEvent::Boot } }
        execute {
            payload: (Command, Command),
            routes: {
                primary: SpacecraftComputerEvent::Execute(payload.0),
                backup: SpacecraftComputerEvent::Execute(payload.1),
            }
        }
        complete_primary { routes: { primary: SpacecraftComputerEvent::Complete } }
        diagnose_primary { routes: { primary: SpacecraftComputerEvent::Diagnose } }
        suspend_primary { routes: { primary: SpacecraftComputerEvent::SafeMode } }
        resume_deep { routes: { primary: SpacecraftComputerEvent::ResumeDeep } }
        resume_shallow { routes: { primary: SpacecraftComputerEvent::ResumeShallow } }
        resume_both { routes: { primary: SpacecraftComputerEvent::Resume, backup: SpacecraftComputerEvent::Resume } }
        complete { routes: { primary: SpacecraftComputerEvent::Complete, backup: SpacecraftComputerEvent::Complete } }
        reset { routes: { primary: SpacecraftComputerEvent::Reset, backup: SpacecraftComputerEvent::Reset } }
        retire { routes: { primary: SpacecraftComputerEvent::Retire, backup: SpacecraftComputerEvent::Retire } }
    }
}

pub async fn run_demo() {
    println!("\n=== Native regions, owned snapshots and independent history ===");
    let mut console = MissionConsole::new(
        DynamicSpacecraftComputer::new(()),
        DynamicSpacecraftComputer::new(()),
        8,
    );
    console.handle(MissionConsoleEvent::Boot).await.unwrap();
    // No Clone implementation on Command: each route consumes its own value.
    console
        .handle(MissionConsoleEvent::Execute((
            Command {
                source: CommandSource::Pilot,
                priority: 4,
            },
            Command {
                source: CommandSource::MissionControl,
                priority: 9,
            },
        )))
        .await
        .unwrap();
    console
        .handle(MissionConsoleEvent::CompletePrimary)
        .await
        .unwrap();
    console
        .handle(MissionConsoleEvent::DiagnosePrimary)
        .await
        .unwrap();
    console
        .handle(MissionConsoleEvent::SuspendPrimary)
        .await
        .unwrap();
    assert_eq!(
        console.current_state().primary,
        SpacecraftComputerState::SafeMode
    );
    assert_eq!(
        console.current_state().backup,
        SpacecraftComputerState::Processing
    );

    let json = serde_json::to_string_pretty(&console.into_snapshot()).unwrap();
    println!("One named envelope:\n{json}");
    let mut snapshot: MissionConsoleSnapshot = serde_json::from_str(&json).unwrap();
    // A migration failure returns the WHOLE owned envelope, including backup's
    // active command. Repair and reuse it; do not clone/reconstruct its regions.
    snapshot.backup.version = 2;
    let (mut snapshot, error) = MissionConsole::from_snapshot(snapshot, 8).err().unwrap();
    assert_matches!(
        error,
        SnapshotError::UnsupportedVersion {
            expected: 1,
            actual: 2
        }
    );
    snapshot.backup.version = 1;
    let mut console = MissionConsole::from_snapshot(snapshot, 8).ok().unwrap();
    assert!(console.take_join().is_none());
    assert_eq!(
        console
            .backup()
            .runner()
            .machine()
            .processing_data()
            .unwrap()
            .0
            .as_ref()
            .unwrap()
            .priority,
        9
    );

    console
        .handle(MissionConsoleEvent::ResumeDeep)
        .await
        .unwrap();
    assert_eq!(
        console.current_state().primary,
        SpacecraftComputerState::Diagnostics
    );
    // A fork is NOT a transaction: primary resumes successfully, backup rejects.
    assert_matches!(
        console.handle(MissionConsoleEvent::ResumeBoth).await,
        Err(ParallelError::Right {
            left_committed: true,
            ..
        })
    );
    assert_eq!(
        console.current_state().primary,
        SpacecraftComputerState::Active
    );
    assert_eq!(
        console.current_state().backup,
        SpacecraftComputerState::Processing
    );

    console
        .handle(MissionConsoleEvent::DiagnosePrimary)
        .await
        .unwrap();
    console
        .handle(MissionConsoleEvent::SuspendPrimary)
        .await
        .unwrap();
    console
        .handle(MissionConsoleEvent::ResumeShallow)
        .await
        .unwrap();
    assert_eq!(
        console.current_state().primary,
        SpacecraftComputerState::Processing,
        "shallow history restores Work's initial child, not Diagnostics"
    );
    assert!(
        console
            .primary()
            .runner()
            .machine()
            .processing_data()
            .unwrap()
            .0
            .is_none(),
        "history restores control, not the earlier owned command"
    );
    console.handle(MissionConsoleEvent::Complete).await.unwrap();

    // Same composition, reset in place: qualified scope generations advance.
    let visit = console.scope_epoch("primary/Operational").unwrap();
    console.handle(MissionConsoleEvent::Reset).await.unwrap();
    assert_matches!(console.scope_epoch("primary/Operational"), Some(epoch) if epoch != visit);
    console.handle(MissionConsoleEvent::Retire).await.unwrap();
    assert!(console.is_finished());
    assert_matches!(
        console.take_join(),
        Some(MissionConsoleState {
            primary: SpacecraftComputerState::Retired,
            backup: SpacecraftComputerState::Retired,
        })
    );
    assert!(console.take_join().is_none());
    let mut restored = MissionConsole::from_snapshot(console.into_snapshot(), 8)
        .ok()
        .unwrap();
    assert!(restored.is_finished());
    assert!(
        restored.take_join().is_none(),
        "restore does not synthesize a join"
    );
    println!("Deep/shallow history, partial fork progress, reset and one-shot join preserved.");
}

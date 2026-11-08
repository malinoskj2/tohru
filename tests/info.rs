//! Exercise the real feature module under the package's nested module layout.
#[path = "../runtime/info/mod.rs"]
pub mod info;

#[test]
fn public_snapshot_preserves_bounded_partial_observation_contract() {
    let snapshot = info::snapshot();
    assert!(snapshot.accounts.len() <= 256);
    assert!(snapshot.disks.len() <= 64);
    assert!(snapshot.mounts.len() <= 128);
    assert!(snapshot.generations.len() <= 256);
    assert!(snapshot.issues.len() <= 32);
    if let Some(current) = snapshot.system.current_generation {
        assert!(
            snapshot
                .generations
                .iter()
                .any(|generation| generation.current && generation.number == current)
        );
    }
}

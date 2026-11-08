use super::*;

#[test]
fn only_confirmed_cycle_can_begin_authentication() {
    let cycle = Cycle::requested(1);
    assert!(cycle.active() && !cycle.held() && cycle.snapshot().is_none());
    cycle.confirm();
    assert!(cycle.held() && cycle.snapshot().is_some());
    cycle.finish();
    assert!(!cycle.active() && cycle.snapshot().is_none());
    cycle.confirm();
    assert!(!cycle.held());
}

#[test]
fn denial_and_account_failure_cannot_mint_release() {
    let cycle = Cycle::requested(1);
    cycle.confirm();
    assert!(cycle.snapshot().unwrap().verified(false).is_none());
    assert!(cycle.held());
}

#[test]
fn finishing_while_pam_blocks_revokes_snapshot() {
    let cycle = Cycle::requested(1);
    cycle.confirm();
    let snapshot = cycle.snapshot().unwrap();
    cycle.finish();
    assert!(snapshot.verified(true).is_none());
}

#[test]
fn permit_is_bound_to_exact_cycle_even_when_owner_index_is_reused() {
    let first = Cycle::requested(1);
    first.confirm();
    let permit = first.snapshot().unwrap().verified(true).unwrap();
    let second = Cycle::requested(1);
    second.confirm();
    assert!(second.reserve(permit).is_none());
    let permit = first.snapshot().unwrap().verified(true).unwrap();
    let release = first.reserve(permit).unwrap();
    assert!(first.accepts_release(&release));
    assert!(first.snapshot().is_none());
    first.finish();
    assert!(!first.accepts_release(&release));
}

#[test]
fn owner_and_other_process_cycles_cannot_share_release() {
    let owner = Cycle::requested(1);
    owner.confirm();
    let permit = owner.snapshot().unwrap().verified(true).unwrap();
    let other = Cycle::requested(2);
    other.confirm();
    assert!(other.reserve(permit).is_none());
}

#[test]
fn reservation_is_one_shot_and_revoked_by_finish() {
    let cycle = Cycle::requested(1);
    cycle.confirm();
    let first = cycle.snapshot().unwrap().verified(true).unwrap();
    let second = cycle.snapshot().unwrap().verified(true).unwrap();
    let release = cycle.reserve(first).unwrap();
    assert!(cycle.reserve(second).is_none());
    assert!(cycle.active() && !cycle.held());
    cycle.finish();
    assert!(!cycle.accepts_release(&release));
}

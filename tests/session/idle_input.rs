use super::{Cancellation, Input};
use crate::idle::{Action, Config, Effect, Policy, Subscription};
use std::time::{Duration, Instant};
fn token(effects: &[Effect]) -> Subscription {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Arm {
                subscription,
                after: Some(_),
            } if subscription.action == Action::Lock => Some(*subscription),
            _ => None,
        })
        .unwrap()
}
#[test]
fn inhibit_release_before_one_drain_cancels_fade_and_queued_task() {
    let now = Instant::now();
    let cancellation = Cancellation::default();
    let mut input = Input::new(cancellation.clone());
    let mut policy = Policy::new(Config::default(), false).unwrap();
    input.update(|state| state.held = Some(false));
    let old = token(&input.drain(&mut policy, now));
    policy.idled(old, now).unwrap();
    assert!(policy.fading());
    let queued_task = cancellation.stamp();
    input.update(|state| state.inhibitors.screen_saver_cookies = 1);
    input.update(|state| state.inhibitors.screen_saver_cookies = 0);
    assert!(!cancellation.current(queued_task)); // Before drain, actual worker gate.
    let effects = input.drain(&mut policy, now);
    assert!(effects.contains(&Effect::ClearFade));
    assert!(!policy.fading());
    assert!(policy.idled(old, now).unwrap().is_empty());
    assert!(
        policy
            .step(now + Duration::from_secs(10))
            .unwrap()
            .is_empty()
    );
    assert_ne!(old, token(&effects));
}
#[test]
fn independent_inhibitor_and_caffeine_pulse_cannot_enable_old_work() {
    let now = Instant::now();
    let cancellation = Cancellation::default();
    let mut input = Input::new(cancellation.clone());
    let mut policy = Policy::new(Config::default(), false).unwrap();
    input.update(|state| {
        state.held = Some(false);
        state.inhibitors.logind_idle = true;
    });
    assert!(
        input
            .drain(&mut policy, now)
            .iter()
            .all(|effect| !matches!(effect, Effect::Arm { after: Some(_), .. }))
    );
    let queued = cancellation.stamp();
    input.update(|state| state.inhibitors.caffeine = true);
    input.update(|state| state.inhibitors.caffeine = false);
    assert!(!cancellation.current(queued));
    assert!(
        input
            .drain(&mut policy, now)
            .iter()
            .all(|effect| !matches!(effect, Effect::Arm { after: Some(_), .. }))
    );
    input.update(|state| state.inhibitors.logind_idle = false);
    assert!(
        input
            .drain(&mut policy, now)
            .iter()
            .any(|effect| matches!(effect, Effect::Arm { after: Some(_), .. }))
    );
    assert!(input.drain(&mut policy, now).is_empty());
}

#[test]
fn held_transition_preserves_pending_sleep_instead_of_treating_it_as_input() {
    let now = Instant::now();
    let cancellation = Cancellation::default();
    let mut input = Input::new(cancellation);
    let config = Config {
        fade: Duration::ZERO,
        ..Config::default()
    };
    let mut policy = Policy::new(config, false).unwrap();
    input.update(|state| state.held = Some(false));
    let arms = input.drain(&mut policy, now);
    let suspend = arms
        .iter()
        .find_map(|effect| match effect {
            Effect::Arm {
                subscription,
                after: Some(_),
            } if subscription.action == Action::Suspend => Some(*subscription),
            _ => None,
        })
        .unwrap();
    assert!(
        policy
            .idled(suspend, now)
            .unwrap()
            .contains(&Effect::RequestLock)
    );
    input.update(|state| state.held = Some(true));
    assert!(
        input
            .drain(&mut policy, now)
            .contains(&Effect::RequestSuspend)
    );
    assert!(input.drain(&mut policy, now).is_empty());
}

#[test]
fn producer_after_decision_snapshot_cannot_rebrand_stale_action_as_current() {
    let cancellation = Cancellation::default();
    let mut input = Input::new(cancellation.clone());
    let mut policy = Policy::new(Config::default(), false).unwrap();
    input.update(|state| state.held = Some(false));
    let _ = input.drain(&mut policy, Instant::now());
    let decision = cancellation.stamp();
    input.update(|state| state.inhibitors.caffeine = true);
    assert!(cancellation.advance(decision).is_none());
    assert!(!cancellation.current(decision));
    let current = cancellation.stamp();
    let next = cancellation.advance(current).unwrap();
    assert!(cancellation.current(next));
    assert!(!cancellation.current(current));
}

use super::*;

fn armed(config: Config, locked: bool) -> (Policy, Vec<Effect>) {
    let mut policy = Policy::new(config, locked).unwrap();
    let effects = policy.arm();
    (policy, effects)
}

fn subscription(effects: &[Effect], action: Action) -> Subscription {
    effects
        .iter()
        .find_map(|effect| match effect {
            Effect::Arm { subscription, .. } if subscription.action == action => {
                Some(*subscription)
            }
            _ => None,
        })
        .unwrap()
}

fn no_fade() -> Config {
    Config {
        fade: Duration::ZERO,
        ..Config::default()
    }
}

#[test]
fn default_timers_preserve_unlocked_and_locked_reference_values() {
    let (policy, effects) = armed(Config::default(), false);
    assert!(policy.deadline().is_none());
    let timeouts: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Arm { after, .. } => Some(*after),
            _ => None,
        })
        .collect();
    assert_eq!(
        timeouts,
        [
            Some(Duration::from_secs(1800)),
            None,
            Some(Duration::from_secs(7200))
        ]
    );
    let (_, effects) = armed(Config::default(), true);
    let timeouts: Vec<_> = effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Arm { after, .. } => Some(*after),
            _ => None,
        })
        .collect();
    assert_eq!(
        timeouts,
        [
            Some(Duration::from_secs(1800)),
            Some(Duration::from_secs(60)),
            Some(Duration::from_secs(5400))
        ]
    );
}

#[test]
fn input_during_fade_invalidates_callbacks_and_never_fires() {
    let (mut policy, effects) = armed(Config::default(), false);
    let timer = subscription(&effects, Action::Suspend);
    let now = Instant::now();
    assert_eq!(
        policy.idled(timer, now).unwrap(),
        [Effect::Fade {
            over: Duration::from_secs(2)
        }]
    );
    assert!(policy.resumed().contains(&Effect::ClearFade));
    assert!(policy.idled(timer, now).unwrap().is_empty());
    assert!(
        policy
            .step(now + Duration::from_secs(10))
            .unwrap()
            .is_empty()
    );
    assert!(policy.deadline().is_none());
}

#[test]
fn coalesced_lock_and_suspend_wait_for_actual_acknowledgment() {
    let (mut policy, effects) = armed(Config::default(), false);
    let now = Instant::now();
    policy
        .idled(subscription(&effects, Action::Lock), now)
        .unwrap();
    policy
        .idled(subscription(&effects, Action::Suspend), now)
        .unwrap();
    let fired = policy.step(now + Duration::from_secs(2)).unwrap();
    assert_eq!(fired, [Effect::RequestLock]);
    assert!(!fired.contains(&Effect::RequestSuspend));
    let held = policy.session(true);
    assert!(held.contains(&Effect::ClearFade));
    assert_eq!(held.last(), Some(&Effect::RequestSuspend));
    assert!(policy.session(true).is_empty());
    assert!(policy.deadline().is_none());
}

#[test]
fn failed_or_late_lock_acknowledgment_cannot_suspend() {
    for failure in [false, true] {
        let (mut policy, effects) = armed(no_fade(), false);
        let now = Instant::now();
        assert_eq!(
            policy
                .idled(subscription(&effects, Action::Suspend), now)
                .unwrap(),
            [Effect::RequestLock]
        );
        let cancelled = if failure {
            policy.lock_failed()
        } else {
            policy.step(now + LOCK_ACK_LIMIT).unwrap()
        };
        assert!(!cancelled.contains(&Effect::RequestSuspend));
        assert!(!policy.session(true).contains(&Effect::RequestSuspend));
        assert!(policy.deadline().is_none());
    }
}

#[test]
fn independent_inhibitors_cancel_pending_work_until_final_release() {
    let (mut policy, effects) = armed(no_fade(), false);
    let old = subscription(&effects, Action::Suspend);
    policy.idled(old, Instant::now()).unwrap();
    let blocked = policy.inhibited(Inhibitors {
        caffeine: true,
        logind_idle: true,
        screen_saver_cookies: 2,
    });
    assert!(
        blocked
            .iter()
            .all(|effect| matches!(effect, Effect::Arm { after: None, .. }))
    );
    assert!(
        policy
            .inhibited(Inhibitors {
                caffeine: false,
                logind_idle: true,
                screen_saver_cookies: 0
            })
            .is_empty()
    );
    assert!(!policy.session(true).contains(&Effect::RequestSuspend));
    let released = policy.inhibited(Inhibitors::default());
    assert!(
        released
            .iter()
            .any(|effect| matches!(effect, Effect::Arm { after: Some(_), .. }))
    );
    assert!(policy.idled(old, Instant::now()).unwrap().is_empty());
}

#[test]
fn screen_off_and_resume_restore_screens_once_without_unlocking() {
    let (mut policy, effects) = armed(no_fade(), true);
    let timer = subscription(&effects, Action::ScreenOff);
    assert_eq!(
        policy.idled(timer, Instant::now()).unwrap(),
        [Effect::ScreensOff]
    );
    assert!(policy.idled(timer, Instant::now()).unwrap().is_empty());
    assert_eq!(policy.resumed().first(), Some(&Effect::ScreensOn));
    assert!(!policy.resumed().contains(&Effect::ScreensOn));
    assert!(policy.locked);
}

#[test]
fn locked_fade_waits_for_cancellation_without_obscuring_authentication() {
    let (mut policy, effects) = armed(Config::default(), true);
    let now = Instant::now();
    assert!(
        policy
            .idled(subscription(&effects, Action::Suspend), now)
            .unwrap()
            .is_empty()
    );
    assert_eq!(policy.deadline(), Some(now + Duration::from_secs(2)));
    assert_eq!(
        policy.step(now + Duration::from_secs(2)).unwrap(),
        [Effect::RequestSuspend]
    );
}

#[test]
fn malformed_config_is_rejected_before_any_observer_is_armed() {
    assert!(matches!(
        Policy::new(
            Config {
                fade: Duration::from_secs(121),
                ..Config::default()
            },
            false
        ),
        Err(Error::InvalidFade)
    ));
    for timeout in [
        Duration::ZERO,
        Duration::from_nanos(1),
        Duration::from_millis(u64::from(u32::MAX) + 1),
    ] {
        let config = Config {
            lock: Timeouts {
                unlocked: Some(timeout),
                locked: None,
            },
            ..Config::default()
        };
        assert!(matches!(
            Policy::new(config, false),
            Err(Error::InvalidTimeout)
        ));
    }
}

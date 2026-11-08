use super::{Change, Configuration};
use crate::idle::{Config, Effect, Policy};
use std::time::{Duration, Instant};

#[test]
fn identical_config_preserves_fade_and_subscription_changed_config_rearms() {
    let config = Config::default();
    let now = Instant::now();
    let mut accepted = Configuration::new(config);
    let mut policy = Policy::new(config, false).unwrap();
    let original = policy
        .arm()
        .iter()
        .find_map(|effect| match effect {
            Effect::Arm {
                subscription,
                after: Some(_),
            } => Some(*subscription),
            _ => None,
        })
        .unwrap();
    policy.idled(original, now).unwrap();
    let opacity = policy.opacity(now + Duration::from_secs(1));
    let deadline = policy.deadline();
    assert_eq!(
        accepted
            .update(config, |_| -> Result<(), &str> {
                panic!("identical config must not enqueue or publish resume")
            })
            .unwrap(),
        Change::Unchanged
    );
    assert!(policy.configure(config).unwrap().is_empty());
    assert!(policy.fading());
    assert_eq!(policy.opacity(now + Duration::from_secs(1)), opacity);
    assert_eq!(policy.deadline(), deadline);
    let changed = Config {
        fade: Duration::from_millis(100),
        ..config
    };
    let mut effects = Vec::new();
    assert_eq!(
        accepted
            .update(changed, |config| {
                effects = policy.configure(config).unwrap();
                Ok::<(), &str>(())
            })
            .unwrap(),
        Change::Accepted
    );
    assert!(!policy.fading());
    assert!(effects.contains(&Effect::ClearFade));
    assert!(policy.idled(original, now).unwrap().is_empty());
}

#[test]
fn failed_config_submission_is_not_cached_and_identical_retry_submits() {
    let config = Config::default();
    let changed = Config {
        fade: Duration::ZERO,
        ..config
    };
    let mut accepted = Configuration::new(config);
    let mut calls = 0;
    assert_eq!(
        accepted.update(changed, |_| {
            calls += 1;
            Err("queue full")
        }),
        Err("queue full")
    );
    assert_eq!(
        accepted
            .update(changed, |_| {
                calls += 1;
                Ok::<(), &str>(())
            })
            .unwrap(),
        Change::Accepted
    );
    assert_eq!(
        accepted
            .update(changed, |_| {
                calls += 1;
                Ok::<(), &str>(())
            })
            .unwrap(),
        Change::Unchanged
    );
    assert_eq!(calls, 2);
}

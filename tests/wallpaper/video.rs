use super::*;
fn snapshot() -> Snapshot {
    Snapshot {
        enabled: true,
        revision: 1,
        assignments: BTreeMap::from([("DP-1".into(), "/chosen/video.mp4".into())]),
        logind: Protection::Clear,
        locker: Protection::Clear,
        occupancy: BTreeMap::from([("DP-1".into(), Occupancy::Empty)]),
    }
}
#[test]
fn only_connected_enabled_assigned_outputs_own_players() {
    let mut value = snapshot();
    assert!(matches!(
        value.action("DP-1", true),
        Action::Keep {
            playback: Playback::Playing,
            ..
        }
    ));
    assert_eq!(value.action("DP-1", false), Action::Stop);
    assert_eq!(value.action("OTHER", true), Action::Stop);
    value.enabled = false;
    assert_eq!(value.action("DP-1", true), Action::Stop);
}
#[test]
fn both_real_lock_observations_and_empty_visible_workspace_are_required() {
    for logind in [
        Protection::Unknown,
        Protection::Clear,
        Protection::Protected,
    ] {
        for locker in [
            Protection::Unknown,
            Protection::Clear,
            Protection::Protected,
        ] {
            for occupancy in [Occupancy::Unknown, Occupancy::Empty, Occupancy::Occupied] {
                let mut value = snapshot();
                value.logind = logind;
                value.locker = locker;
                value.occupancy.insert("DP-1".into(), occupancy);
                let playing = matches!(
                    value.action("DP-1", true),
                    Action::Keep {
                        playback: Playback::Playing,
                        ..
                    }
                );
                assert_eq!(
                    playing,
                    logind == Protection::Clear
                        && locker == Protection::Clear
                        && occupancy == Occupancy::Empty
                );
            }
        }
    }
    let mut value = snapshot();
    value.occupancy.clear();
    assert!(matches!(
        value.action("DP-1", true),
        Action::Keep {
            playback: Playback::Paused(PauseReason::UnknownOccupancy),
            ..
        }
    ));
}
#[test]
fn invalid_proposals_do_not_enter_the_backend_contract() {
    let mut value = snapshot();
    assert!(value.validate().is_ok());
    value
        .assignments
        .insert("DP-1\u{a0}".into(), "/chosen/video.mp4".into());
    assert_eq!(value.validate(), Err(Error::Output));
    value = snapshot();
    value
        .assignments
        .insert("DP-1".into(), "/chosen/image.png".into());
    assert_eq!(value.validate(), Err(Error::Path));
    value = snapshot();
    for i in 0..32 {
        value
            .assignments
            .insert(format!("OTHER{i}"), "/chosen/video.mp4".into());
    }
    assert_eq!(value.validate(), Err(Error::Capacity));
}
#[test]
fn failed_players_back_off_to_a_bounded_retry_interval() {
    let now = Instant::now();
    let mut retry = Backoff::failed(None, now);
    assert_eq!(retry.retry_at() - now, Duration::from_millis(100));
    for _ in 0..100 {
        retry = Backoff::failed(Some(retry), now);
    }
    assert_eq!(retry.retry_at() - now, Duration::from_secs(10));
}

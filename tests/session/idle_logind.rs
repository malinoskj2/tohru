use super::{Owner, owner_signal};
#[test]
fn authenticated_owner_route_handles_loss_and_replacement_with_fresh_query_only() {
    let route = |sender, name| {
        owner_signal(
            sender,
            "org.freedesktop.DBus",
            "NameOwnerChanged",
            "/org/freedesktop/DBus",
            name,
        )
    };
    assert!(route("org.freedesktop.DBus", "org.freedesktop.login1"));
    assert!(!route(":1.42", "org.freedesktop.login1"));
    assert!(!route("org.freedesktop.DBus", "other.service"));
    let mut owner = Owner::default();
    let initial = owner.revision();
    owner.initialize(initial, Some(":1.1"));
    let previous = owner.query().unwrap();
    assert_eq!(owner.complete(&previous, Some(false)), Some(false));
    owner.changed("");
    assert!(owner.query().is_none());
    assert_eq!(owner.complete(&previous, Some(false)), None);
    owner.initialize(initial, Some(":1.1"));
    assert!(owner.query().is_none());
    owner.changed(":1.2");
    let replacement = owner.query().unwrap();
    assert_eq!(owner.complete(&replacement, None), Some(true));
    let fresh = owner.query().unwrap();
    assert_eq!(owner.complete(&fresh, Some(false)), Some(false));
    assert_eq!(owner.complete(&replacement, Some(false)), None);
}

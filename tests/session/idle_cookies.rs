use super::*;

#[test]
fn unique_client_owns_its_cookie_and_disconnect_releases_only_its_own() {
    let mut cookies = Cookies::default();
    let first = cookies.inhibit(":1.1").unwrap();
    let second = cookies.inhibit(":1.2").unwrap();
    assert_eq!(cookies.release(":1.2", first), Release::DifferentOwner);
    assert_eq!(cookies.release(":1.1", u32::MAX), Release::Unknown);
    cookies.client_gone(":1.1");
    assert_eq!(cookies.count(), 1);
    assert_eq!(cookies.release(":1.2", second), Release::Released);
    assert_eq!(cookies.count(), 0);
}

#[test]
fn client_and_total_capacity_are_enforced_without_growing_on_rejection() {
    let mut cookies = Cookies::default();
    for _ in 0..PER_CLIENT {
        cookies.inhibit(":1.1").unwrap();
    }
    assert_eq!(cookies.inhibit(":1.1"), Err(Refused::ClientLimit));
    assert_eq!(cookies.count(), PER_CLIENT);
    for index in 2..=8 {
        for _ in 0..PER_CLIENT {
            cookies.inhibit(&format!(":1.{index}")).unwrap();
        }
    }
    assert_eq!(cookies.count(), TOTAL);
    assert_eq!(cookies.inhibit(":1.9"), Err(Refused::TotalLimit));
    cookies.client_gone(":1.1");
    assert!(cookies.inhibit(":1.9").is_ok());
}

#[test]
fn wrap_skips_reserved_zero_and_live_cookie_collisions() {
    let mut cookies = Cookies {
        next: 1,
        ..Cookies::default()
    };
    let first = cookies.inhibit(":1.1").unwrap();
    assert_eq!(first, 1);
    cookies.next = u32::MAX;
    assert_eq!(cookies.inhibit(":1.2").unwrap(), u32::MAX);
    assert_eq!(cookies.inhibit(":1.3").unwrap(), 2);
}

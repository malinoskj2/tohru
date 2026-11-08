#[test]
fn auth_unlock_uses_immutable_root_plugin_boundary_not_alias() {
    use crate::logic::Script;
    let (tx, _rx) = std::sync::mpsc::channel();
    let (events, _) = std::sync::mpsc::channel();
    let blocked = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut script = super::LuauScript::new(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/session/security-owner.plm"
        ),
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/session/security-owner.luau"
        ),
        tx.clone(),
        events,
        blocked.clone(),
    );
    let _ = script.scene();
    // This is the immutable native discriminator set by for_plugin; the alias
    // deliberately resembles the ordinary locker name and confers no authority.
    script.prefix = Some("tohru-lock".into());
    let mut ctx = crate::logic::Context::for_plugin(tx, blocked);
    script.on_event(crate::scene::Event::Alarm("start"), &mut ctx);
    let error: String = script
        .lua
        .as_ref()
        .unwrap()
        .globals()
        .get("owner_error")
        .unwrap();
    assert!(error.contains("only the native root locker"), "{error}");
}

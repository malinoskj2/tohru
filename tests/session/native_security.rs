use super::*;
use crate::scene::{Animated, Ctx};

#[test]
fn plugin_authentication_cannot_enter_native_pam_or_mint_permit() {
    let (tx, rx) = std::sync::mpsc::channel();
    let error = authenticate_unlock(false, "not-a-real-secret", &tx).unwrap_err();
    assert!(error.to_string().contains("only the native root locker"));
    assert!(rx.try_recv().is_err());
}

#[test]
fn secret_wait_operands_and_diagnostics_use_native_metadata() {
    let (scene, _) = crate::language::read_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/session/security-secret.plm"
    ))
    .unwrap();
    for name in ["security_password", "security_answer"] {
        assert!(secret(name));
        assert!(crate::agent::Cond::parse(&scene, &format!("{name} == \"probe\" 1ms")).is_err());
        assert!(crate::agent::Cond::parse(&scene, &format!("not {name} 1ms")).is_err());
    }
    assert!(!secret("security_public"));
    assert!(crate::agent::Cond::parse(&scene, "security_public == \"ok\" 1ms").is_ok());
}

#[test]
fn queued_public_wait_is_rejected_when_current_metadata_becomes_secret() {
    let (mut scene, _) = crate::language::read_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/session/security-public.plm"
    ))
    .unwrap();
    let (cond, _) =
        crate::agent::Cond::parse(&scene, "security_later_private has \"probe\"").unwrap();
    // Same metadata publication the compiler uses when a reloaded binding is secret.
    crate::scene::SECRETS
        .lock()
        .unwrap()
        .push("security_later_private");
    let props: Vec<Animated> = scene
        .props
        .iter()
        .map(|(_, v, spring)| Animated::at(*v, *spring))
        .collect();
    let facts: Vec<f32> = scene.facts.iter().map(|(_, v)| *v).collect();
    let texts: Vec<String> = scene
        .texts
        .iter()
        .map(|(_, _)| "private-probe-value".into())
        .collect();
    let ctx = Ctx {
        props: &props,
        facts: &facts,
        seen: None,
    };
    assert!(cond.reads_secret(&scene));
    assert!(!cond.holds(&scene, ctx, &texts));
    assert_eq!(cond.now(&scene, ctx, &texts), "secret input cannot be read");
    // Missing/reordered index is rejected rather than evaluated against another text.
    scene.texts.clear();
    assert!(cond.reads_secret(&scene));
}

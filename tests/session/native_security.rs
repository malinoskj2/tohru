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

#[test]
fn human_watch_and_action_diff_cannot_reveal_old_secret_after_text_reordering() {
    let (mut scene, _) = crate::language::read_file(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/session/security-secret.plm"
    ))
    .unwrap();
    let props: Vec<Animated> = scene
        .props
        .iter()
        .map(|(_, v, spring)| Animated::at(*v, *spring))
        .collect();
    let facts: Vec<f32> = scene.facts.iter().map(|(_, v)| *v).collect();
    let mut texts: Vec<String> = scene.texts.iter().map(|(_, value)| value.clone()).collect();
    let k = scene
        .texts
        .iter()
        .position(|(name, _)| *name == "security_password")
        .unwrap();
    texts[k] = "synthetic-old-secret".into();
    let ctx = Ctx {
        props: &props,
        facts: &facts,
        seen: None,
    };
    let before = crate::agent::before(&scene, ctx, &texts, Vec::new());
    scene.texts[k].0 = "security_rebound_public";
    texts[k] = "public text".into();
    let diff = crate::agent::what_happened(&scene, &before, ctx, &texts, &[], &[]);
    assert!(!diff.contains("synthetic-old-secret"), "{diff}");
    assert!(diff.contains("changed (hidden)"), "{diff}");
}

#[test]
fn active_native_owner_remains_open_for_culling_input_and_pacing() {
    assert!(surface_open(1, false, &[1]));
    assert!(surface_open(1, true, &[1]));
    assert!(!surface_open(2, false, &[1]));
    assert!(surface_open(2, true, &[1]));
    assert!(!surface_open(1, false, &[]));
}

#[test]
fn queued_retired_target_keeps_native_owner_until_gpu_target_is_destroyed() {
    use std::sync::{Arc, Mutex, Weak};
    struct Owner(Arc<Mutex<Vec<&'static str>>>);
    impl Drop for Owner {
        fn drop(&mut self) {
            self.0.lock().unwrap().push("native owner");
        }
    }
    struct Target {
        native: Weak<Owner>,
        events: Arc<Mutex<Vec<&'static str>>>,
    }
    impl Drop for Target {
        fn drop(&mut self) {
            assert!(
                self.native.upgrade().is_some(),
                "GPU target outlived native surface owner"
            );
            self.events.lock().unwrap().push("gpu target");
        }
    }
    impl crate::gpu::Frames for Target {
        fn acquire(&mut self, _: &wgpu::Device, _: &[u64]) -> Option<(usize, wgpu::Texture)> {
            unreachable!()
        }
        fn present(
            &mut self,
            _: usize,
            _: wgpu::SubmissionIndex,
            _: &wgpu::Device,
            _: &wgpu::Queue,
            _: Option<[u32; 4]>,
        ) {
            unreachable!()
        }
    }
    struct Window {
        _native: Arc<Owner>,
        cycle: authority::Cycle,
    }
    impl crate::platform::PlatformWindow for Window {
        fn is_current(&self) -> bool {
            self.cycle.publishable()
        }
        fn update_input_region(&self, _: &[[i32; 4]]) {}
        fn cursor(&self, _: crate::scene::Cursor) {}
        fn keyboard(&self, _: crate::scene::Keyboard) {}
    }
    let events = Arc::new(Mutex::new(Vec::new()));
    let native = Arc::new(Owner(events.clone()));
    let cycle = authority::Cycle::requested(1);
    cycle.confirm();
    let (send, receive) = std::sync::mpsc::channel();
    send.send(crate::gpu::NewSheet {
        id: 1,
        target: crate::gpu::Target::Frames(Box::new(Target {
            native: Arc::downgrade(&native),
            events: events.clone(),
        })),
        window: Box::new(Window {
            _native: native.clone(),
            cycle: cycle.clone(),
        }),
        scale: 1.0,
        size: (100, 100),
        mhz: 60000,
        name: "queued-lock-face".into(),
        view: crate::gpu::View {
            surface: 1,
            popup: None,
            origin: (0.0, 0.0),
            size: (100.0, 100.0),
        },
    })
    .unwrap();
    let permit = cycle.snapshot().unwrap().verified(true).unwrap();
    let _release = cycle.reserve(permit).unwrap();
    drop(native); // Engaged's owner reference can leave while the target is queued.
    let queued = receive.recv().unwrap();
    assert!(
        !queued.window.is_current(),
        "releasing targets must not install"
    );
    drop(queued); // Actual native NewSheet field order: target before window owner.
    assert_eq!(*events.lock().unwrap(), ["gpu target", "native owner"]);
}

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use tohru_shell::settings::{Cli, Key, LockStyle, Origin, Owner, Palette, Value};

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Fixture(PathBuf);

impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "tohru-settings-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn user(&self) -> PathBuf {
        self.0.join("config.toml")
    }

    fn managed(&self) -> PathBuf {
        self.0.join("managed.toml")
    }

    fn owner(&self) -> Owner {
        Owner::load(self.user(), Some(self.managed())).unwrap()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn corner_size_bounds_atomic_reload_and_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "# retained\n[appearance]\nfuture_key = 'kept'\n",
    )
    .unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(32));
    for invalid in [
        Value::Integer(0),
        Value::Integer(101),
        Value::Bool(true),
        Value::Text("32".into()),
    ] {
        let before = fs::read(files.user()).unwrap();
        assert!(owner.set(Key::CornerSize, invalid).is_err());
        assert_eq!(fs::read(files.user()).unwrap(), before);
        assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(32));
    }
    owner.set(Key::CornerSize, Value::Integer(100)).unwrap();
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::CornerSize).value, Value::Integer(100));
    let written = fs::read_to_string(files.user()).unwrap();
    assert!(written.contains("# retained") && written.contains("future_key = 'kept'"));
    fs::write(files.user(), "[appearance]\ncorner_size = 100.5\n").unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(100));
    fs::write(files.user(), written).unwrap();
    fs::write(files.managed(), "[appearance]\ncorner_size = 1\n").unwrap();
    let mut managed = files.owner();
    managed.set(Key::CornerSize, Value::Integer(80)).unwrap();
    managed.reload().unwrap();
    assert_eq!(managed.get(Key::CornerSize).value, Value::Integer(80));
    assert!(managed.get(Key::CornerSize).runtime_override);
    assert_eq!(files.owner().get(Key::CornerSize).value, Value::Integer(1));
}

#[test]
fn precedence_reload_and_restart() {
    let files = Fixture::new();
    fs::write(
        files.user(),
        "[bar]\nmooncoil = true\npreferred_output = 'DP-1'\n",
    )
    .unwrap();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    let mut owner = files.owner();
    assert_eq!(owner.get(Key::Corners).origin, Origin::Default);
    assert_eq!(owner.get(Key::PreferredOutput).origin, Origin::User);
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Nix);
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    owner.reload().unwrap();
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
    assert!(owner.get(Key::Mooncoil).managed);
    assert!(owner.get(Key::Mooncoil).runtime_override);
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(true));
    assert_eq!(
        fs::read_to_string(files.managed()).unwrap(),
        "[bar]\nmooncoil = false\n"
    );
    // A separate OS process loads Nix values, never the parent's override.
    let result = Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "restart_child", "--nocapture"])
        .env("TOHRU_TEST_RESTART_DIR", &files.0)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn restart_child() {
    let Some(path) = std::env::var_os("TOHRU_TEST_RESTART_DIR") else {
        return;
    };
    let path = PathBuf::from(path);
    let owner = Owner::load(path.join("config.toml"), Some(path.join("managed.toml"))).unwrap();
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(false));
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Nix);
}

#[test]
fn unmanaged_writes_preserve_future_keys_comments_and_external_edits() {
    let files = Fixture::new();
    let mut owner = Owner::load(files.user(), None).unwrap();
    fs::write(
        files.user(),
        "# keep this comment\n[future]\nvalue = [1,2,3]\n[bar]\npreferred_output = 'HDMI-A-1'\n",
    )
    .unwrap();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    let written = fs::read_to_string(files.user()).unwrap();
    assert!(written.contains("# keep this comment"));
    assert!(written.contains("value = [1,2,3]"));
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(true));
    assert_eq!(
        restarted.get(Key::PreferredOutput).value,
        Value::Text("HDMI-A-1".into())
    );
}

#[test]
fn invalid_reload_is_atomic_and_does_not_discard_override() {
    let files = Fixture::new();
    fs::write(files.user(), "[bar]\npreferred_output = 'DP-1'\n").unwrap();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    let mut owner = files.owner();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    fs::write(files.user(), "[bar]\npreferred_output = 'DP-2'\n").unwrap();
    fs::write(files.managed(), "[appearance]\ncorners = 'invalid'\n").unwrap();
    assert!(owner.reload().is_err());
    assert_eq!(
        owner.get(Key::PreferredOutput).value,
        Value::Text("DP-1".into())
    );
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    owner.reload().unwrap();
    assert_eq!(
        owner.get(Key::PreferredOutput).value,
        Value::Text("DP-2".into())
    );
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Runtime);
}

#[test]
fn invalid_values_and_failed_writes_do_not_apply() {
    let files = Fixture::new();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Text("bad".into())).is_err());
    assert!(
        owner
            .set(Key::PreferredOutput, Value::Text("bad\noutput".into()))
            .is_err()
    );
    fs::create_dir(files.user()).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Bool(true)).is_err());
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Default);
}

#[cfg(unix)]
#[test]
fn symlink_to_managed_is_never_rewritten() {
    let files = Fixture::new();
    fs::write(files.managed(), "[bar]\nmooncoil = false\n").unwrap();
    std::os::unix::fs::symlink(files.managed(), files.user()).unwrap();
    assert!(Owner::load(files.user(), Some(files.managed())).is_err());
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(owner.set(Key::Mooncoil, Value::Bool(true)).is_err());
    assert_eq!(
        fs::read_to_string(files.managed()).unwrap(),
        "[bar]\nmooncoil = false\n"
    );
}

#[test]
fn oversized_and_missing_managed_files_are_errors() {
    let files = Fixture::new();
    assert!(Owner::load(files.user(), Some(files.managed())).is_err());
    fs::write(files.user(), "#".repeat(1024 * 1024 + 1)).unwrap();
    assert!(Owner::load(files.user(), None).is_err());
}

#[test]
fn serialized_size_limit_rejects_growth_without_changing_disk_or_memory() {
    let files = Fixture::new();
    let original = format!("#{}\n", "x".repeat(1024 * 1024 - 2));
    fs::write(files.user(), &original).unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    assert!(matches!(
        owner.set(Key::Mooncoil, Value::Bool(true)),
        Err(tohru_shell::settings::Error::TooLarge(_))
    ));
    assert_eq!(fs::read_to_string(files.user()).unwrap(), original);
    assert_eq!(owner.get(Key::Mooncoil).origin, Origin::Default);
    assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(false));
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(false));
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 1);

    // An edit whose resulting document is exactly at the limit remains usable.
    let suffix = "[bar]\nmooncoil = true\n";
    let prefix = format!("#{}\n", "x".repeat(1024 * 1024 - suffix.len() - 2));
    fs::write(files.user(), &prefix).unwrap();
    let mut owner = Owner::load(files.user(), None).unwrap();
    owner.set(Key::Mooncoil, Value::Bool(true)).unwrap();
    assert_eq!(fs::metadata(files.user()).unwrap().len(), 1024 * 1024);
    let restarted = Owner::load(files.user(), None).unwrap();
    assert_eq!(restarted.get(Key::Mooncoil).value, Value::Bool(true));
}

#[test]
fn palette_is_typed_canonical_and_restartable() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    let mut owner = files.owner();
    assert_eq!(
        owner.get(Key::Palette).value,
        Value::Palette(Palette::Mocha)
    );
    for palette in Palette::ALL {
        let parsed = Key::Palette.parse_text(palette.name()).unwrap();
        assert_eq!(parsed, Value::Palette(palette));
        owner.set(Key::Palette, parsed.clone()).unwrap();
        assert_eq!(files.owner().get(Key::Palette).value, parsed);
    }
    for bad in ["rose-pine", "Mocha", ""] {
        assert!(Key::Palette.parse_text(bad).is_err());
    }
    let disk = fs::read(files.user()).unwrap();
    assert!(owner.set(Key::Palette, Value::Text("nord".into())).is_err());
    assert_eq!(fs::read(files.user()).unwrap(), disk);
}

#[test]
fn related_managed_proposals_never_half_persist() {
    use std::collections::BTreeMap;
    let files = Fixture::new();
    fs::write(files.managed(), "[appearance]\npalette='mocha'\n").unwrap();
    fs::write(files.user(), "# retained\n[appearance]\ncorner_size=40\n").unwrap();
    let mut owner = files.owner();
    let disk = fs::read(files.user()).unwrap();
    let proposal = BTreeMap::from([
        (Key::CornerSize, Value::Integer(64)),
        (Key::Palette, Value::Palette(Palette::Nord)),
    ]);
    let mut invalid = proposal.clone();
    invalid.insert(Key::Mooncoil, Value::Text("yes".into()));
    assert!(owner.set_many(invalid).is_err());
    assert_eq!(owner.get(Key::CornerSize).value, Value::Integer(40));
    assert!(matches!(
        owner.set_many(proposal.clone()).unwrap(),
        tohru_shell::settings::SetOutcome::RuntimeOverride
    ));
    owner.reload().unwrap();
    for (key, value) in proposal {
        assert_eq!(owner.get(key).value, value);
        assert!(owner.get(key).runtime_override);
    }
    assert_eq!(fs::read(files.user()).unwrap(), disk);
    let restarted = files.owner();
    assert_eq!(restarted.get(Key::CornerSize).value, Value::Integer(40));
    assert_eq!(
        restarted.get(Key::Palette).value,
        Value::Palette(Palette::Mocha)
    );
}

#[test]
fn writable_batch_commits_together_and_rejects_partial_growth() {
    use std::collections::BTreeMap;
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    fs::write(files.user(), "# keep\n[appearance]\nfuture='kept'\n").unwrap();
    let mut owner = files.owner();
    let proposal = BTreeMap::from([
        (Key::Mooncoil, Value::Bool(true)),
        (Key::CornerSize, Value::Integer(48)),
        (Key::Palette, Value::Palette(Palette::Nord)),
    ]);
    owner.set_many(proposal.clone()).unwrap();
    let restarted = files.owner();
    for (key, value) in &proposal {
        assert_eq!(&restarted.get(*key).value, value);
    }
    let disk = fs::read_to_string(files.user()).unwrap();
    assert!(disk.contains("# keep") && disk.contains("future='kept'"));
    assert!(owner.set_many(BTreeMap::new()).is_err());
    let original = format!("#{}\n", "x".repeat(1024 * 1024 - 2));
    fs::write(files.user(), &original).unwrap();
    let mut rejected = proposal.clone();
    rejected.insert(Key::Palette, Value::Palette(Palette::Mocha));
    rejected.insert(Key::PreferredOutput, Value::Text("DP-9".into()));
    assert!(matches!(
        owner.set_many(rejected),
        Err(tohru_shell::settings::Error::TooLarge(_))
    ));
    assert_eq!(fs::read_to_string(files.user()).unwrap(), original);
    for (key, value) in &proposal {
        assert_eq!(&owner.get(*key).value, value);
    }
    assert_eq!(fs::read_dir(&files.0).unwrap().count(), 2);
    fs::write(files.user(), "[appearance]\npalette='invalid'\n").unwrap();
    assert!(owner.reload().is_err());
    for (key, value) in proposal {
        assert_eq!(owner.get(key).value, value);
    }
}

#[test]
fn lock_style_defaults_to_flooded_monolith_and_round_trips() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    let mut owner = files.owner();
    let setting = owner.get(Key::LockStyle);
    assert_eq!(setting.value, Value::LockStyle(LockStyle::FloodedMonolith));
    assert_eq!(setting.origin, Origin::Default);
    assert!(owner.warnings().is_empty());
    let names: Vec<_> = LockStyle::ALL.iter().map(|style| style.name()).collect();
    assert_eq!(
        names,
        [
            "still-glass",
            "ribbon-key",
            "flooded-monolith",
            "drifting-frost-veil",
            "unfolding-glass-capsule",
            "edge-glass-drawer",
            "horizon-meniscus",
            "refractive-typographic-lens",
            "corner-anchor",
            "bar-drop-curtain",
        ]
    );
    for style in LockStyle::ALL {
        let parsed = Key::LockStyle.parse_text(style.name()).unwrap();
        owner.set(Key::LockStyle, parsed.clone()).unwrap();
        assert_eq!(files.owner().get(Key::LockStyle).value, parsed);
    }
    assert!(
        fs::read_to_string(files.user())
            .unwrap()
            .contains("[lock]\nstyle = \"bar-drop-curtain\"")
    );
    for bad in ["Flooded-Monolith", "flooded_monolith", "plain", ""] {
        assert!(Key::LockStyle.parse_text(bad).is_err());
    }
}

#[test]
fn unknown_lock_style_warns_and_uses_the_default_without_failing_the_load() {
    let files = Fixture::new();
    fs::write(files.managed(), "[lock]\nstyle = 7\n").unwrap();
    fs::write(
        files.user(),
        "[lock]\nstyle = \"aurora\"\n[appearance]\npalette = \"nord\"\n",
    )
    .unwrap();
    let owner = files.owner();
    assert_eq!(
        owner.get(Key::LockStyle).value,
        Value::LockStyle(LockStyle::FloodedMonolith)
    );
    assert_eq!(owner.get(Key::LockStyle).origin, Origin::Default);
    assert_eq!(owner.get(Key::Palette).value, Value::Palette(Palette::Nord));
    let warnings = owner.warnings();
    assert_eq!(warnings.len(), 2, "{warnings:?}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("\"aurora\"") && w.contains("flooded-monolith"))
    );
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("managed.toml") && w.contains('7'))
    );

    fs::write(files.managed(), "[lock]\nstyle = \"corner-anchor\"\n").unwrap();
    let owner = files.owner();
    assert_eq!(
        owner.get(Key::LockStyle).value,
        Value::LockStyle(LockStyle::CornerAnchor)
    );
    assert_eq!(owner.get(Key::LockStyle).origin, Origin::Nix);
    assert_eq!(owner.warnings().len(), 1);
}

#[test]
fn notifications_position_defaults_to_top_right_and_round_trips() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    let mut owner = files.owner();
    let setting = owner.get(Key::NotificationsPosition);
    assert_eq!(setting.value, Value::Text("top-right".into()));
    assert_eq!(setting.origin, Origin::Default);
    for position in ["bottom-right", "top-right"] {
        let parsed = Key::NotificationsPosition.parse_text(position).unwrap();
        owner
            .set(Key::NotificationsPosition, parsed.clone())
            .unwrap();
        assert_eq!(files.owner().get(Key::NotificationsPosition).value, parsed);
    }
    assert!(
        fs::read_to_string(files.user())
            .unwrap()
            .contains("[notifications]\nposition = \"top-right\"")
    );
    for bad in ["top-left", "Top-Right", "bottom_right", ""] {
        let before = fs::read(files.user()).unwrap();
        assert!(
            owner
                .set(Key::NotificationsPosition, Value::Text(bad.into()))
                .is_err()
        );
        assert_eq!(fs::read(files.user()).unwrap(), before);
    }
    fs::write(files.user(), "[notifications]\nposition = \"left\"\n").unwrap();
    assert!(Owner::load(files.user(), Some(files.managed())).is_err());
    fs::write(
        files.managed(),
        "[notifications]\nposition = \"bottom-right\"\n",
    )
    .unwrap();
    fs::write(files.user(), "").unwrap();
    let owner = files.owner();
    assert_eq!(
        owner.get(Key::NotificationsPosition).value,
        Value::Text("bottom-right".into())
    );
    assert_eq!(owner.get(Key::NotificationsPosition).origin, Origin::Nix);
}

#[test]
fn assistant_settings_default_to_voice_on_and_validate() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    let mut owner = files.owner();
    assert_eq!(owner.get(Key::AssistantVoice).value, Value::Bool(true));
    assert_eq!(
        owner.get(Key::AssistantProvider).value,
        Value::Text(String::new())
    );
    for key in [
        Key::AssistantVadModel,
        Key::AssistantAsrModel,
        Key::AssistantTurnModel,
        Key::AssistantTtsModel,
        Key::AssistantKwsModel,
    ] {
        assert_eq!(owner.get(key).value, Value::Text(String::new()));
    }
    assert!(Key::parse("assistant.voice_models").is_err());
    assert_eq!(
        owner.get(Key::AssistantTtsEngine).value,
        Value::Text("kokoro".into())
    );
    assert_eq!(
        owner.get(Key::AssistantTtsVoice).value,
        Value::Text("en-us+f3".into())
    );
    assert_eq!(owner.get(Key::AssistantTtsSpeed).value, Value::Integer(100));
    assert_eq!(owner.get(Key::AssistantTtsPitch).value, Value::Integer(3));
    owner.set(Key::AssistantVoice, Value::Bool(false)).unwrap();
    owner
        .set(Key::AssistantProvider, Value::Text("codex".into()))
        .unwrap();
    owner
        .set(Key::AssistantTtsPitch, Value::Integer(-12))
        .unwrap();
    owner
        .set(
            Key::AssistantAsrModel,
            Value::Text("/models/sherpa-onnx-streaming-zipformer-en-2023-06-21".into()),
        )
        .unwrap();
    let restarted = files.owner();
    assert_eq!(restarted.get(Key::AssistantVoice).value, Value::Bool(false));
    assert_eq!(
        restarted.get(Key::AssistantProvider).value,
        Value::Text("codex".into())
    );
    assert_eq!(
        restarted.get(Key::AssistantTtsPitch).value,
        Value::Integer(-12)
    );
    assert!(
        fs::read_to_string(files.user())
            .unwrap()
            .contains("[assistant]\nvoice = false")
    );
    let rejected = [
        (Key::AssistantProvider, Value::Text("aider".into())),
        (Key::AssistantProvider, Value::Text("Claude".into())),
        (Key::AssistantTtsEngine, Value::Text("say".into())),
        (Key::AssistantTtsVoice, Value::Text("en us; rm".into())),
        (Key::AssistantTtsVoice, Value::Text(String::new())),
        (Key::AssistantVadModel, Value::Text("relative/path".into())),
        (Key::AssistantTurnModel, Value::Text("~/turn.onnx".into())),
        (Key::AssistantTtsSpeed, Value::Integer(10)),
        (
            Key::AssistantEffort(Cli::Codex),
            Value::Text("extreme".into()),
        ),
        (
            Key::AssistantModel(Cli::Claude),
            Value::Text("opus; rm -rf".into()),
        ),
        (Key::AssistantPersonaFile, Value::Text("persona.md".into())),
        (Key::AssistantMemory, Value::Text("off".into())),
        (
            Key::AssistantTtsVoicepack,
            Value::Text("../../etc/passwd".into()),
        ),
        (
            Key::AssistantTtsVoicepack,
            Value::Text("relative/pack.bin".into()),
        ),
        (Key::AssistantVoiceWake, Value::Text("always".into())),
        (Key::AssistantOrca, Value::Text("yes".into())),
        (Key::AssistantOrca, Value::Integer(1)),
        (Key::AssistantTtsModel, Value::Text("kokoro".into())),
        (Key::AssistantTtsPitch, Value::Integer(13)),
        (Key::AssistantVoice, Value::Text("true".into())),
    ];
    for (key, bad) in rejected {
        let before = fs::read(files.user()).unwrap();
        assert!(owner.set(key, bad).is_err(), "{key:?}");
        assert_eq!(fs::read(files.user()).unwrap(), before);
    }
    fs::write(files.managed(), "[assistant]\nvoice = true\n").unwrap();
    let owner = files.owner();
    assert_eq!(owner.get(Key::AssistantVoice).value, Value::Bool(true));
    assert_eq!(owner.get(Key::AssistantVoice).origin, Origin::Nix);
}

#[test]
fn assistant_speech_defaults_to_kokoro_with_the_tide_pack() {
    let files = Fixture::new();
    fs::write(files.managed(), "").unwrap();
    let mut owner = files.owner();
    assert_eq!(
        owner.get(Key::AssistantTtsVoicepack).value,
        Value::Text("tide".into())
    );
    assert_eq!(
        owner.get(Key::AssistantTtsLoadOnConversation).value,
        Value::Bool(false)
    );
    assert_eq!(
        owner.get(Key::AssistantVoiceWake).value,
        Value::Text("asr".into())
    );
    owner
        .set(Key::AssistantTtsVoicepack, Value::Text("af_heart".into()))
        .unwrap();
    owner
        .set(Key::AssistantVoiceWake, Value::Text("kws".into()))
        .unwrap();
    let restarted = files.owner();
    assert_eq!(
        restarted.get(Key::AssistantTtsVoicepack).value,
        Value::Text("af_heart".into())
    );
    assert_eq!(
        restarted.get(Key::AssistantVoiceWake).value,
        Value::Text("kws".into())
    );
}

#[test]
fn assistant_models_nest_under_each_provider() {
    let files = Fixture::new();
    fs::write(
        files.managed(),
        "[assistant.models.gemini]\nmodel = \"gemini-4-neon\"\neffort = \"high\"\n",
    )
    .unwrap();
    let mut owner = files.owner();
    assert_eq!(owner.get(Key::AssistantPersona).value, Value::Bool(true));
    assert_eq!(owner.get(Key::AssistantMemory).value, Value::Bool(true));
    assert_eq!(
        owner.get(Key::AssistantModel(Cli::Claude)).value,
        Value::Text(String::new())
    );
    assert_eq!(
        owner.get(Key::AssistantModel(Cli::Gemini)).value,
        Value::Text("gemini-4-neon".into())
    );
    assert_eq!(
        owner.get(Key::AssistantEffort(Cli::Gemini)).origin,
        Origin::Nix
    );
    owner
        .set(
            Key::AssistantModel(Cli::Codex),
            Value::Text("gpt-6.2-sol".into()),
        )
        .unwrap();
    owner
        .set(
            Key::AssistantEffort(Cli::Codex),
            Value::Text("xhigh".into()),
        )
        .unwrap();
    owner.set(Key::AssistantMemory, Value::Bool(false)).unwrap();
    let written = fs::read_to_string(files.user()).unwrap();
    assert!(written.contains("[assistant.models.codex]"), "{written}");
    let restarted = files.owner();
    assert_eq!(
        restarted.get(Key::AssistantModel(Cli::Codex)).value,
        Value::Text("gpt-6.2-sol".into())
    );
    assert_eq!(
        restarted.get(Key::AssistantEffort(Cli::Codex)).value,
        Value::Text("xhigh".into())
    );
    assert_eq!(
        restarted.get(Key::AssistantMemory).value,
        Value::Bool(false)
    );
    assert_eq!(
        Key::parse("assistant.models.opencode.effort").unwrap(),
        Key::AssistantEffort(Cli::Opencode)
    );
    assert_eq!(
        Key::parse("assistant.orca").unwrap(),
        Key::AssistantOrca
    );
    assert_eq!(
        restarted.get(Key::AssistantOrca).value,
        Value::Text("auto".into())
    );
    owner
        .set(Key::AssistantOrca, Value::Text("false".into()))
        .unwrap();
    let restarted = files.owner();
    assert_eq!(
        restarted.get(Key::AssistantOrca).value,
        Value::Text("false".into())
    );
    fs::write(files.managed(), "[assistant]\norca = true\n").unwrap();
    let bool_managed = files.owner();
    assert_eq!(
        bool_managed.get(Key::AssistantOrca).value,
        Value::Text("true".into())
    );
    assert_eq!(bool_managed.get(Key::AssistantOrca).origin, Origin::Nix);
}

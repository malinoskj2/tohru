//! Layered TOML settings whose managed overrides live outside scene reloads.
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use thiserror::Error;
use toml_edit::{DocumentMut, Item, Table, value};

const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Key {
    ShortcutsMode,
    ShortcutLauncher,
    ShortcutSession,
    ShortcutAssistant,
    FontText,
    FontDisplay,
    FontMono,
    WallpaperEnabled,
    WallpaperBackend,
    WallpaperDirectory,
    WallpaperAssignments,
    WallpaperExecutable,
    WallpaperOptions,
    IdleOwner,
    IdleLockSeconds,
    IdleSuspendSeconds,
    VoicePython,
    VoiceEnvironment,
    PreferredOutput,
    Mooncoil,
    InfoButton,
    AboutButton,
    Corners,
    CornerSize,
    Palette,
    LockStyle,
    PolkitAgent,
    NotificationsPosition,
    AssistantVoice,
    AssistantReducedMotion,
    AssistantVoiceSource,
    AssistantProvider,
    AssistantVadModel,
    AssistantAsrModel,
    AssistantTurnModel,
    AssistantTtsEngine,
    AssistantTtsVoice,
    AssistantTtsSpeed,
    AssistantTtsPitch,
    AssistantPersona,
    AssistantPersonaFile,
    AssistantMemory,
    AssistantOrca,
    AssistantDiscordEnabled,
    AssistantDiscordRead,
    AssistantDiscordPost,
    AssistantDiscordDesktopFirst,
    AssistantDiscordDesktopTimeoutSeconds,
    AssistantDiscordTokenFile,
    AssistantDiscordGuildId,
    AssistantDiscordAllowedUserId,
    AssistantDiscordChannels,
    AssistantTtsModel,
    AssistantTtsVoicepack,
    AssistantTtsLoadOnConversation,
    AssistantVoiceWake,
    AssistantKwsModel,
    AssistantBackend,
    AssistantOrcaBot,
    AssistantOrcaVoiceChat,
    AssistantOrcaOffline,
    AssistantModel(Cli),
    AssistantEffort(Cli),
    AssistantVoiceModel(Cli),
    AssistantVoiceEffort(Cli),
    AssistantSemanticModel(Cli),
    AssistantSemanticEffort(Cli),
}

fn valid_discord_id(text: &str) -> bool {
    (1..=20).contains(&text.len()) && text.bytes().all(|byte| byte.is_ascii_digit())
}

/// A provider CLI with its own model and effort settings.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Cli {
    Claude,
    Codex,
    Gemini,
    Agy,
    Opencode,
}

/// Where notification toasts stack: `top-right` under the bar, or `bottom-right`.
pub const NOTIFICATION_POSITIONS: [&str; 2] = ["top-right", "bottom-right"];

/// The provider CLIs the assistant knows, in palette order; empty means the first one signed in.
pub const ASSISTANT_PROVIDERS: [&str; 6] = ["", "claude", "codex", "gemini", "agy", "opencode"];

/// Effort levels any provider CLI accepts; empty means Tohru's default for that provider.
pub const ASSISTANT_EFFORTS: [&str; 7] = ["", "minimal", "low", "medium", "high", "xhigh", "max"];

/// Speech engines the voice service can run.
pub const TTS_ENGINES: [&str; 2] = ["kokoro", "espeak-ng"];

/// How the voice service spots the wake word: a keyword spotter, or the full recogniser.
pub const VOICE_WAKE: [&str; 2] = ["kws", "asr"];

/// Whether the assistant gets Orca CLI context: detect when installed, always on, or off.
pub const ASSISTANT_ORCA: [&str; 3] = ["auto", "true", "false"];

/// Which backend runs assistant chats: local provider CLIs (default) or an opt-in Orca bot.
pub const ASSISTANT_BACKENDS: [&str; 2] = ["local", "orca"];

/// Where voice conversations land when `assistant.backend = "orca"`.
pub const ORCA_VOICE_CHATS: [&str; 2] = ["home", "new"];

/// What happens when `assistant.backend = "orca"` and Orca is unreachable.
pub const ORCA_OFFLINE_MODES: [&str; 2] = ["error", "local"];

impl Key {
    pub const ALL: [Self; 91] = [
        Self::ShortcutsMode,
        Self::ShortcutLauncher,
        Self::ShortcutSession,
        Self::ShortcutAssistant,
        Self::FontText,
        Self::FontDisplay,
        Self::FontMono,
        Self::WallpaperEnabled,
        Self::WallpaperBackend,
        Self::WallpaperDirectory,
        Self::WallpaperAssignments,
        Self::WallpaperExecutable,
        Self::WallpaperOptions,
        Self::IdleOwner,
        Self::IdleLockSeconds,
        Self::IdleSuspendSeconds,
        Self::VoicePython,
        Self::VoiceEnvironment,
        Self::PreferredOutput,
        Self::Mooncoil,
        Self::InfoButton,
        Self::AboutButton,
        Self::Corners,
        Self::CornerSize,
        Self::Palette,
        Self::LockStyle,
        Self::PolkitAgent,
        Self::NotificationsPosition,
        Self::AssistantVoice,
        Self::AssistantReducedMotion,
        Self::AssistantVoiceSource,
        Self::AssistantProvider,
        Self::AssistantVadModel,
        Self::AssistantAsrModel,
        Self::AssistantTurnModel,
        Self::AssistantTtsEngine,
        Self::AssistantTtsVoice,
        Self::AssistantTtsSpeed,
        Self::AssistantTtsPitch,
        Self::AssistantPersona,
        Self::AssistantPersonaFile,
        Self::AssistantMemory,
        Self::AssistantOrca,
        Self::AssistantDiscordEnabled,
        Self::AssistantDiscordRead,
        Self::AssistantDiscordPost,
        Self::AssistantDiscordDesktopFirst,
        Self::AssistantDiscordDesktopTimeoutSeconds,
        Self::AssistantDiscordTokenFile,
        Self::AssistantDiscordGuildId,
        Self::AssistantDiscordAllowedUserId,
        Self::AssistantDiscordChannels,
        Self::AssistantTtsModel,
        Self::AssistantTtsVoicepack,
        Self::AssistantTtsLoadOnConversation,
        Self::AssistantVoiceWake,
        Self::AssistantKwsModel,
        Self::AssistantBackend,
        Self::AssistantOrcaBot,
        Self::AssistantOrcaVoiceChat,
        Self::AssistantOrcaOffline,
        Self::AssistantModel(Cli::Claude),
        Self::AssistantEffort(Cli::Claude),
        Self::AssistantModel(Cli::Codex),
        Self::AssistantEffort(Cli::Codex),
        Self::AssistantModel(Cli::Gemini),
        Self::AssistantEffort(Cli::Gemini),
        Self::AssistantModel(Cli::Agy),
        Self::AssistantEffort(Cli::Agy),
        Self::AssistantModel(Cli::Opencode),
        Self::AssistantEffort(Cli::Opencode),
        Self::AssistantVoiceModel(Cli::Claude),
        Self::AssistantVoiceEffort(Cli::Claude),
        Self::AssistantVoiceModel(Cli::Codex),
        Self::AssistantVoiceEffort(Cli::Codex),
        Self::AssistantVoiceModel(Cli::Gemini),
        Self::AssistantVoiceEffort(Cli::Gemini),
        Self::AssistantVoiceModel(Cli::Agy),
        Self::AssistantVoiceEffort(Cli::Agy),
        Self::AssistantVoiceModel(Cli::Opencode),
        Self::AssistantVoiceEffort(Cli::Opencode),
        Self::AssistantSemanticModel(Cli::Claude),
        Self::AssistantSemanticEffort(Cli::Claude),
        Self::AssistantSemanticModel(Cli::Codex),
        Self::AssistantSemanticEffort(Cli::Codex),
        Self::AssistantSemanticModel(Cli::Gemini),
        Self::AssistantSemanticEffort(Cli::Gemini),
        Self::AssistantSemanticModel(Cli::Agy),
        Self::AssistantSemanticEffort(Cli::Agy),
        Self::AssistantSemanticModel(Cli::Opencode),
        Self::AssistantSemanticEffort(Cli::Opencode),
    ];

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::ShortcutsMode => "shortcuts.mode",
            Self::ShortcutLauncher => "shortcuts.launcher_trigger",
            Self::ShortcutSession => "shortcuts.session_trigger",
            Self::ShortcutAssistant => "shortcuts.assistant_trigger",
            Self::FontText => "appearance.font_text",
            Self::FontDisplay => "appearance.font_display",
            Self::FontMono => "appearance.font_mono",
            Self::WallpaperEnabled => "wallpaper.enabled",
            Self::WallpaperBackend => "wallpaper.backend",
            Self::WallpaperDirectory => "wallpaper.directory",
            Self::WallpaperAssignments => "wallpaper.assignments",
            Self::WallpaperExecutable => "wallpaper.executable",
            Self::WallpaperOptions => "wallpaper.options",
            Self::IdleOwner => "idle.owner",
            Self::IdleLockSeconds => "idle.lock_seconds",
            Self::IdleSuspendSeconds => "idle.suspend_seconds",
            Self::VoicePython => "assistant.voice_python",
            Self::VoiceEnvironment => "assistant.voice_environment",
            Self::PreferredOutput => "bar.preferred_output",
            Self::Mooncoil => "bar.mooncoil",
            Self::InfoButton => "bar.info_button",
            Self::AboutButton => "bar.about_button",
            Self::Corners => "appearance.corners",
            Self::CornerSize => "appearance.corner_size",
            Self::Palette => "appearance.palette",
            Self::LockStyle => "lock.style",
            Self::PolkitAgent => "polkit.agent",
            Self::NotificationsPosition => "notifications.position",
            Self::AssistantVoice => "assistant.voice",
            Self::AssistantReducedMotion => "assistant.reduced_motion",
            Self::AssistantVoiceSource => "assistant.voice_source",
            Self::AssistantProvider => "assistant.provider",
            Self::AssistantVadModel => "assistant.vad_model",
            Self::AssistantAsrModel => "assistant.asr_model",
            Self::AssistantTurnModel => "assistant.turn_model",
            Self::AssistantTtsEngine => "assistant.tts_engine",
            Self::AssistantTtsVoice => "assistant.tts_voice",
            Self::AssistantTtsSpeed => "assistant.tts_speed",
            Self::AssistantTtsPitch => "assistant.tts_pitch",
            Self::AssistantPersona => "assistant.persona",
            Self::AssistantPersonaFile => "assistant.persona_file",
            Self::AssistantMemory => "assistant.memory",
            Self::AssistantOrca => "assistant.orca",
            Self::AssistantDiscordEnabled => "assistant.discord.enabled",
            Self::AssistantDiscordRead => "assistant.discord.read",
            Self::AssistantDiscordPost => "assistant.discord.post",
            Self::AssistantDiscordDesktopFirst => "assistant.discord.desktop_first",
            Self::AssistantDiscordDesktopTimeoutSeconds => {
                "assistant.discord.desktop_timeout_seconds"
            }
            Self::AssistantDiscordTokenFile => "assistant.discord.token_file",
            Self::AssistantDiscordGuildId => "assistant.discord.guild_id",
            Self::AssistantDiscordAllowedUserId => "assistant.discord.allowed_user_id",
            Self::AssistantDiscordChannels => "assistant.discord.channels",
            Self::AssistantTtsModel => "assistant.tts_model",
            Self::AssistantTtsVoicepack => "assistant.tts_voicepack",
            Self::AssistantTtsLoadOnConversation => "assistant.tts_load_on_conversation",
            Self::AssistantVoiceWake => "assistant.voice_wake",
            Self::AssistantKwsModel => "assistant.kws_model",
            Self::AssistantBackend => "assistant.backend",
            Self::AssistantOrcaBot => "assistant.orca_bot",
            Self::AssistantOrcaVoiceChat => "assistant.orca_voice_chat",
            Self::AssistantOrcaOffline => "assistant.orca_offline",
            Self::AssistantModel(Cli::Claude) => "assistant.models.claude.model",
            Self::AssistantEffort(Cli::Claude) => "assistant.models.claude.effort",
            Self::AssistantModel(Cli::Codex) => "assistant.models.codex.model",
            Self::AssistantEffort(Cli::Codex) => "assistant.models.codex.effort",
            Self::AssistantModel(Cli::Gemini) => "assistant.models.gemini.model",
            Self::AssistantEffort(Cli::Gemini) => "assistant.models.gemini.effort",
            Self::AssistantModel(Cli::Agy) => "assistant.models.agy.model",
            Self::AssistantEffort(Cli::Agy) => "assistant.models.agy.effort",
            Self::AssistantModel(Cli::Opencode) => "assistant.models.opencode.model",
            Self::AssistantEffort(Cli::Opencode) => "assistant.models.opencode.effort",
            Self::AssistantVoiceModel(Cli::Claude) => "assistant.voice_models.claude.model",
            Self::AssistantVoiceEffort(Cli::Claude) => "assistant.voice_models.claude.effort",
            Self::AssistantVoiceModel(Cli::Codex) => "assistant.voice_models.codex.model",
            Self::AssistantVoiceEffort(Cli::Codex) => "assistant.voice_models.codex.effort",
            Self::AssistantVoiceModel(Cli::Gemini) => "assistant.voice_models.gemini.model",
            Self::AssistantVoiceEffort(Cli::Gemini) => "assistant.voice_models.gemini.effort",
            Self::AssistantVoiceModel(Cli::Agy) => "assistant.voice_models.agy.model",
            Self::AssistantVoiceEffort(Cli::Agy) => "assistant.voice_models.agy.effort",
            Self::AssistantVoiceModel(Cli::Opencode) => "assistant.voice_models.opencode.model",
            Self::AssistantVoiceEffort(Cli::Opencode) => "assistant.voice_models.opencode.effort",
            Self::AssistantSemanticModel(Cli::Claude) => "assistant.semantic_models.claude.model",
            Self::AssistantSemanticEffort(Cli::Claude) => "assistant.semantic_models.claude.effort",
            Self::AssistantSemanticModel(Cli::Codex) => "assistant.semantic_models.codex.model",
            Self::AssistantSemanticEffort(Cli::Codex) => "assistant.semantic_models.codex.effort",
            Self::AssistantSemanticModel(Cli::Gemini) => "assistant.semantic_models.gemini.model",
            Self::AssistantSemanticEffort(Cli::Gemini) => "assistant.semantic_models.gemini.effort",
            Self::AssistantSemanticModel(Cli::Agy) => "assistant.semantic_models.agy.model",
            Self::AssistantSemanticEffort(Cli::Agy) => "assistant.semantic_models.agy.effort",
            Self::AssistantSemanticModel(Cli::Opencode) => {
                "assistant.semantic_models.opencode.model"
            }
            Self::AssistantSemanticEffort(Cli::Opencode) => {
                "assistant.semantic_models.opencode.effort"
            }
        }
    }

    /// Parse a supported setting name.
    ///
    /// # Errors
    /// Returns an error for keys not owned by this schema.
    pub fn parse(name: &str) -> Result<Self, Error> {
        Self::ALL
            .into_iter()
            .find(|key| key.name() == name)
            .ok_or_else(|| Error::UnknownKey(name.to_owned()))
    }

    /// Convert canonical string input at a TOML/Lua boundary.
    ///
    /// # Errors
    /// Rejects unknown closed-domain names; `Owner` validates other text bounds.
    pub fn parse_text(self, text: &str) -> Result<Value, Error> {
        match self {
            Self::Palette => Palette::parse(text)
                .map(Value::Palette)
                .ok_or(Error::InvalidValue(self.name())),
            Self::LockStyle => LockStyle::parse(text)
                .map(Value::LockStyle)
                .ok_or(Error::InvalidValue(self.name())),
            _ => Ok(Value::Text(text.to_owned())),
        }
    }

    #[must_use]
    pub fn parse_bool(self, boolean: bool) -> Value {
        match self {
            // Accept unquoted TOML/Lua booleans for tri-state assistant.orca alongside "auto"/"true"/"false".
            Self::AssistantOrca => Value::Text(if boolean { "true" } else { "false" }.to_owned()),
            _ => Value::Bool(boolean),
        }
    }

    /// The tables leading to the setting and its field.
    const fn location(self) -> (&'static [&'static str], &'static str) {
        match self {
            Self::ShortcutsMode => (&["shortcuts"], "mode"),
            Self::ShortcutLauncher => (&["shortcuts"], "launcher_trigger"),
            Self::ShortcutSession => (&["shortcuts"], "session_trigger"),
            Self::ShortcutAssistant => (&["shortcuts"], "assistant_trigger"),
            Self::FontText => (&["appearance"], "font_text"),
            Self::FontDisplay => (&["appearance"], "font_display"),
            Self::FontMono => (&["appearance"], "font_mono"),
            Self::WallpaperEnabled => (&["wallpaper"], "enabled"),
            Self::WallpaperBackend => (&["wallpaper"], "backend"),
            Self::WallpaperDirectory => (&["wallpaper"], "directory"),
            Self::WallpaperAssignments => (&["wallpaper"], "assignments"),
            Self::WallpaperExecutable => (&["wallpaper"], "executable"),
            Self::WallpaperOptions => (&["wallpaper"], "options"),
            Self::IdleOwner => (&["idle"], "owner"),
            Self::IdleLockSeconds => (&["idle"], "lock_seconds"),
            Self::IdleSuspendSeconds => (&["idle"], "suspend_seconds"),
            Self::VoicePython => (&["assistant"], "voice_python"),
            Self::VoiceEnvironment => (&["assistant"], "voice_environment"),
            Self::PreferredOutput => (&["bar"], "preferred_output"),
            Self::Mooncoil => (&["bar"], "mooncoil"),
            Self::InfoButton => (&["bar"], "info_button"),
            Self::AboutButton => (&["bar"], "about_button"),
            Self::Corners => (&["appearance"], "corners"),
            Self::CornerSize => (&["appearance"], "corner_size"),
            Self::Palette => (&["appearance"], "palette"),
            Self::LockStyle => (&["lock"], "style"),
            Self::PolkitAgent => (&["polkit"], "agent"),
            Self::NotificationsPosition => (&["notifications"], "position"),
            Self::AssistantVoice => (&["assistant"], "voice"),
            Self::AssistantReducedMotion => (&["assistant"], "reduced_motion"),
            Self::AssistantVoiceSource => (&["assistant"], "voice_source"),
            Self::AssistantProvider => (&["assistant"], "provider"),
            Self::AssistantVadModel => (&["assistant"], "vad_model"),
            Self::AssistantAsrModel => (&["assistant"], "asr_model"),
            Self::AssistantTurnModel => (&["assistant"], "turn_model"),
            Self::AssistantTtsEngine => (&["assistant"], "tts_engine"),
            Self::AssistantTtsVoice => (&["assistant"], "tts_voice"),
            Self::AssistantTtsSpeed => (&["assistant"], "tts_speed"),
            Self::AssistantTtsPitch => (&["assistant"], "tts_pitch"),
            Self::AssistantPersona => (&["assistant"], "persona"),
            Self::AssistantPersonaFile => (&["assistant"], "persona_file"),
            Self::AssistantMemory => (&["assistant"], "memory"),
            Self::AssistantOrca => (&["assistant"], "orca"),
            Self::AssistantDiscordEnabled => (&["assistant", "discord"], "enabled"),
            Self::AssistantDiscordRead => (&["assistant", "discord"], "read"),
            Self::AssistantDiscordPost => (&["assistant", "discord"], "post"),
            Self::AssistantDiscordDesktopFirst => (&["assistant", "discord"], "desktop_first"),
            Self::AssistantDiscordDesktopTimeoutSeconds => {
                (&["assistant", "discord"], "desktop_timeout_seconds")
            }
            Self::AssistantDiscordTokenFile => (&["assistant", "discord"], "token_file"),
            Self::AssistantDiscordGuildId => (&["assistant", "discord"], "guild_id"),
            Self::AssistantDiscordAllowedUserId => (&["assistant", "discord"], "allowed_user_id"),
            Self::AssistantDiscordChannels => (&["assistant", "discord"], "channels"),
            Self::AssistantTtsModel => (&["assistant"], "tts_model"),
            Self::AssistantTtsVoicepack => (&["assistant"], "tts_voicepack"),
            Self::AssistantTtsLoadOnConversation => (&["assistant"], "tts_load_on_conversation"),
            Self::AssistantVoiceWake => (&["assistant"], "voice_wake"),
            Self::AssistantKwsModel => (&["assistant"], "kws_model"),
            Self::AssistantBackend => (&["assistant"], "backend"),
            Self::AssistantOrcaBot => (&["assistant"], "orca_bot"),
            Self::AssistantOrcaVoiceChat => (&["assistant"], "orca_voice_chat"),
            Self::AssistantOrcaOffline => (&["assistant"], "orca_offline"),
            Self::AssistantModel(cli) => (cli.tables(), "model"),
            Self::AssistantEffort(cli) => (cli.tables(), "effort"),
            Self::AssistantVoiceModel(Cli::Claude) => {
                (&["assistant", "voice_models", "claude"], "model")
            }
            Self::AssistantVoiceEffort(Cli::Claude) => {
                (&["assistant", "voice_models", "claude"], "effort")
            }
            Self::AssistantVoiceModel(Cli::Codex) => {
                (&["assistant", "voice_models", "codex"], "model")
            }
            Self::AssistantVoiceEffort(Cli::Codex) => {
                (&["assistant", "voice_models", "codex"], "effort")
            }
            Self::AssistantVoiceModel(Cli::Gemini) => {
                (&["assistant", "voice_models", "gemini"], "model")
            }
            Self::AssistantVoiceEffort(Cli::Gemini) => {
                (&["assistant", "voice_models", "gemini"], "effort")
            }
            Self::AssistantVoiceModel(Cli::Agy) => (&["assistant", "voice_models", "agy"], "model"),
            Self::AssistantVoiceEffort(Cli::Agy) => {
                (&["assistant", "voice_models", "agy"], "effort")
            }
            Self::AssistantVoiceModel(Cli::Opencode) => {
                (&["assistant", "voice_models", "opencode"], "model")
            }
            Self::AssistantVoiceEffort(Cli::Opencode) => {
                (&["assistant", "voice_models", "opencode"], "effort")
            }
            Self::AssistantSemanticModel(Cli::Claude) => {
                (&["assistant", "semantic_models", "claude"], "model")
            }
            Self::AssistantSemanticEffort(Cli::Claude) => {
                (&["assistant", "semantic_models", "claude"], "effort")
            }
            Self::AssistantSemanticModel(Cli::Codex) => {
                (&["assistant", "semantic_models", "codex"], "model")
            }
            Self::AssistantSemanticEffort(Cli::Codex) => {
                (&["assistant", "semantic_models", "codex"], "effort")
            }
            Self::AssistantSemanticModel(Cli::Gemini) => {
                (&["assistant", "semantic_models", "gemini"], "model")
            }
            Self::AssistantSemanticEffort(Cli::Gemini) => {
                (&["assistant", "semantic_models", "gemini"], "effort")
            }
            Self::AssistantSemanticModel(Cli::Agy) => {
                (&["assistant", "semantic_models", "agy"], "model")
            }
            Self::AssistantSemanticEffort(Cli::Agy) => {
                (&["assistant", "semantic_models", "agy"], "effort")
            }
            Self::AssistantSemanticModel(Cli::Opencode) => {
                (&["assistant", "semantic_models", "opencode"], "model")
            }
            Self::AssistantSemanticEffort(Cli::Opencode) => {
                (&["assistant", "semantic_models", "opencode"], "effort")
            }
        }
    }

    #[must_use]
    pub const fn integer_bounds(self) -> Option<(i64, i64)> {
        match self {
            Self::IdleLockSeconds | Self::IdleSuspendSeconds => Some((0, 86400)),
            Self::AssistantDiscordDesktopTimeoutSeconds => Some((10, 300)),
            Self::CornerSize => Some((1, 100)),
            Self::AssistantTtsSpeed => Some((50, 200)),
            Self::AssistantTtsPitch => Some((-12, 12)),
            _ => None,
        }
    }

    fn default_value(self) -> Value {
        match self {
            Self::ShortcutsMode => Value::Text("compositor".to_owned()),
            Self::ShortcutLauncher | Self::ShortcutSession | Self::ShortcutAssistant => {
                Value::Text(String::new())
            }
            Self::FontText => Value::Text("Noto Sans".to_owned()),
            Self::FontDisplay => Value::Text("Noto Sans".to_owned()),
            Self::FontMono => Value::Text("DejaVu Sans Mono".to_owned()),
            Self::WallpaperEnabled => Value::Bool(false),
            Self::WallpaperBackend => Value::Text("mpvpaper".to_owned()),
            Self::WallpaperDirectory => Value::Text("".to_owned()),
            Self::WallpaperAssignments => Value::Text("".to_owned()),
            Self::WallpaperExecutable => Value::Text("mpvpaper".to_owned()),
            Self::WallpaperOptions => Value::Text("no-audio loop-file=inf".to_owned()),
            Self::IdleOwner => Value::Text("external".to_owned()),
            Self::IdleLockSeconds => Value::Integer(300),
            Self::IdleSuspendSeconds => Value::Integer(0),
            Self::VoicePython => Value::Text("python3".to_owned()),
            Self::VoiceEnvironment => Value::Text("".to_owned()),
            Self::AssistantDiscordDesktopTimeoutSeconds => Value::Integer(60),
            Self::AssistantDiscordEnabled
            | Self::AssistantDiscordRead
            | Self::AssistantDiscordPost
            | Self::AssistantDiscordDesktopFirst
            | Self::Mooncoil
            | Self::InfoButton
            | Self::AboutButton
            | Self::PolkitAgent
            | Self::AssistantTtsLoadOnConversation
            | Self::AssistantReducedMotion => Value::Bool(false),
            Self::Corners
            | Self::AssistantVoice
            | Self::AssistantPersona
            | Self::AssistantMemory => Value::Bool(true),
            Self::AssistantVoiceSource
            | Self::AssistantDiscordTokenFile
            | Self::AssistantDiscordGuildId
            | Self::AssistantDiscordAllowedUserId
            | Self::AssistantDiscordChannels
            | Self::PreferredOutput
            | Self::AssistantProvider
            | Self::AssistantVadModel
            | Self::AssistantAsrModel
            | Self::AssistantTurnModel
            | Self::AssistantPersonaFile
            | Self::AssistantTtsModel
            | Self::AssistantKwsModel
            | Self::AssistantModel(_)
            | Self::AssistantEffort(_)
            | Self::AssistantVoiceModel(_)
            | Self::AssistantVoiceEffort(_)
            | Self::AssistantSemanticModel(_)
            | Self::AssistantSemanticEffort(_) => Value::Text(String::new()),
            Self::AssistantTtsEngine => Value::Text(TTS_ENGINES[0].to_owned()),
            Self::AssistantTtsVoice => Value::Text("en-us+f3".to_owned()),
            Self::AssistantTtsVoicepack => Value::Text("tide".to_owned()),
            Self::AssistantVoiceWake => Value::Text(VOICE_WAKE[1].to_owned()),
            Self::AssistantOrca => Value::Text(ASSISTANT_ORCA[0].to_owned()),
            Self::AssistantBackend => Value::Text(ASSISTANT_BACKENDS[0].to_owned()),
            Self::AssistantOrcaBot => Value::Text("Tohru".to_owned()),
            Self::AssistantOrcaVoiceChat => Value::Text(ORCA_VOICE_CHATS[0].to_owned()),
            Self::AssistantOrcaOffline => Value::Text(ORCA_OFFLINE_MODES[0].to_owned()),
            Self::AssistantTtsSpeed => Value::Integer(100),
            Self::AssistantTtsPitch => Value::Integer(3),
            Self::CornerSize => Value::Integer(32),
            Self::Palette => Value::Palette(Palette::Mocha),
            Self::LockStyle => Value::LockStyle(LockStyle::DEFAULT),
            Self::NotificationsPosition => Value::Text(NOTIFICATION_POSITIONS[0].to_owned()),
        }
    }

    fn valid_text(self, text: &str) -> bool {
        match self {
            Self::ShortcutsMode if ["compositor", "portal", "disabled"].contains(&text) => true,
            Self::ShortcutLauncher | Self::ShortcutSession | Self::ShortcutAssistant
                if text.len() <= 256 && !text.chars().any(char::is_control) =>
            {
                true
            }
            Self::FontText | Self::FontDisplay | Self::FontMono
                if !text.trim().is_empty()
                    && text.len() <= 256
                    && !text.chars().any(char::is_control)
                    && !text.contains(':') =>
            {
                true
            }
            Self::WallpaperBackend if text == "mpvpaper" => true,
            Self::IdleOwner if ["external", "tohru", "disabled"].contains(&text) => true,
            Self::WallpaperDirectory | Self::WallpaperAssignments | Self::VoiceEnvironment
                if text.len() <= 4096
                    && !text.chars().any(char::is_control)
                    && (text.is_empty() || text.starts_with('/')) =>
            {
                true
            }
            Self::WallpaperExecutable | Self::VoicePython | Self::WallpaperOptions
                if text.len() <= 4096
                    && !text.is_empty()
                    && !text.chars().any(char::is_control) =>
            {
                true
            }
            Self::PreferredOutput if text.len() <= 256 && !text.chars().any(char::is_control) => {
                true
            }
            Self::NotificationsPosition if NOTIFICATION_POSITIONS.contains(&text) => true,
            Self::AssistantDiscordGuildId if text.is_empty() || valid_discord_id(text) => true,
            Self::AssistantDiscordAllowedUserId
                if text.is_empty()
                    || valid_discord_id(text)
                        && !text.starts_with('0')
                        && text.parse::<u64>().is_ok() =>
            {
                true
            }
            Self::AssistantDiscordChannels
                if text.is_empty()
                    || text.len() <= 704
                        && text.split(',').count() <= 32
                        && text.split(',').all(|id| valid_discord_id(id.trim())) =>
            {
                true
            }
            Self::AssistantProvider if ASSISTANT_PROVIDERS.contains(&text) => true,
            Self::AssistantTtsEngine if TTS_ENGINES.contains(&text) => true,
            Self::AssistantEffort(_)
            | Self::AssistantVoiceEffort(_)
            | Self::AssistantSemanticEffort(_)
                if ASSISTANT_EFFORTS.contains(&text) =>
            {
                true
            }
            Self::AssistantModel(_)
            | Self::AssistantVoiceModel(_)
            | Self::AssistantSemanticModel(_)
                if text.len() <= 128
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "._:/@#+-".contains(c)) =>
            {
                true
            }
            Self::AssistantVoiceWake if VOICE_WAKE.contains(&text) => true,
            Self::AssistantOrca if ASSISTANT_ORCA.contains(&text) => true,
            Self::AssistantBackend if ASSISTANT_BACKENDS.contains(&text) => true,
            Self::AssistantOrcaBot
                if (1..=64).contains(&text.len())
                    && !text.starts_with('.')
                    && !text.trim().is_empty()
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || " _.-".contains(c)) =>
            {
                true
            }
            Self::AssistantOrcaVoiceChat if ORCA_VOICE_CHATS.contains(&text) => true,
            Self::AssistantOrcaOffline if ORCA_OFFLINE_MODES.contains(&text) => true,
            Self::AssistantTtsVoicepack
                if (1..=64).contains(&text.len())
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "_.-".contains(c))
                    && !text.starts_with('.')
                    || text.starts_with('/')
                        && text.len() <= 4096
                        && !text.chars().any(char::is_control) =>
            {
                true
            }
            Self::AssistantVadModel
            | Self::AssistantAsrModel
            | Self::AssistantTurnModel
            | Self::AssistantPersonaFile
            | Self::AssistantTtsModel
            | Self::AssistantKwsModel
            | Self::AssistantDiscordTokenFile
                if text.len() <= 4096
                    && !text.chars().any(char::is_control)
                    && (text.is_empty() || text.starts_with('/')) =>
            {
                true
            }
            Self::AssistantVoiceSource
                if text.len() <= 256 && !text.chars().any(char::is_control) =>
            {
                true
            }
            Self::AssistantTtsVoice
                if (1..=64).contains(&text.len())
                    && text
                        .chars()
                        .all(|c| c.is_ascii_alphanumeric() || "+-_.".contains(c)) =>
            {
                true
            }
            _ => false,
        }
    }

    fn validate(self, candidate: &Value) -> Result<(), Error> {
        match (self, candidate) {
            (_, Value::Text(text)) if self.valid_text(text) => Ok(()),
            (
                Self::WallpaperEnabled
                | Self::AssistantDiscordEnabled
                | Self::AssistantDiscordRead
                | Self::AssistantDiscordPost
                | Self::AssistantDiscordDesktopFirst
                | Self::Mooncoil
                | Self::InfoButton
                | Self::AboutButton
                | Self::Corners
                | Self::PolkitAgent
                | Self::AssistantVoice
                | Self::AssistantReducedMotion
                | Self::AssistantPersona
                | Self::AssistantMemory
                | Self::AssistantTtsLoadOnConversation,
                Value::Bool(_),
            )
            | (Self::Palette, Value::Palette(_))
            | (Self::LockStyle, Value::LockStyle(_)) => Ok(()),
            (_, Value::Integer(value))
                if self
                    .integer_bounds()
                    .is_some_and(|(min, max)| (min..=max).contains(value)) =>
            {
                Ok(())
            }
            _ => Err(Error::InvalidValue(self.name())),
        }
    }
}

impl Cli {
    const fn tables(self) -> &'static [&'static str] {
        match self {
            Self::Claude => &["assistant", "models", "claude"],
            Self::Codex => &["assistant", "models", "codex"],
            Self::Gemini => &["assistant", "models", "gemini"],
            Self::Agy => &["assistant", "models", "agy"],
            Self::Opencode => &["assistant", "models", "opencode"],
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Palette {
    Mocha,
    Macchiato,
    RosePine,
    Nord,
    Gruvbox,
    TokyoNight,
    Dracula,
    Everforest,
}

impl Palette {
    pub const ALL: [Self; 8] = [
        Self::Mocha,
        Self::Macchiato,
        Self::RosePine,
        Self::Nord,
        Self::Gruvbox,
        Self::TokyoNight,
        Self::Dracula,
        Self::Everforest,
    ];

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|palette| palette.name() == name)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mocha => "mocha",
            Self::Macchiato => "macchiato",
            Self::RosePine => "rose_pine",
            Self::Nord => "nord",
            Self::Gruvbox => "gruvbox",
            Self::TokyoNight => "tokyo_night",
            Self::Dracula => "dracula",
            Self::Everforest => "everforest",
        }
    }
}

/// The lock screen's look. Every style is the same locker; only its scene differs.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LockStyle {
    StillGlass,
    RibbonKey,
    FloodedMonolith,
    DriftingFrostVeil,
    UnfoldingGlassCapsule,
    EdgeGlassDrawer,
    HorizonMeniscus,
    RefractiveTypographicLens,
    CornerAnchor,
    BarDropCurtain,
}

impl LockStyle {
    pub const DEFAULT: Self = Self::FloodedMonolith;

    pub const ALL: [Self; 10] = [
        Self::StillGlass,
        Self::RibbonKey,
        Self::FloodedMonolith,
        Self::DriftingFrostVeil,
        Self::UnfoldingGlassCapsule,
        Self::EdgeGlassDrawer,
        Self::HorizonMeniscus,
        Self::RefractiveTypographicLens,
        Self::CornerAnchor,
        Self::BarDropCurtain,
    ];

    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|style| style.name() == name)
    }

    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::StillGlass => "still-glass",
            Self::RibbonKey => "ribbon-key",
            Self::FloodedMonolith => "flooded-monolith",
            Self::DriftingFrostVeil => "drifting-frost-veil",
            Self::UnfoldingGlassCapsule => "unfolding-glass-capsule",
            Self::EdgeGlassDrawer => "edge-glass-drawer",
            Self::HorizonMeniscus => "horizon-meniscus",
            Self::RefractiveTypographicLens => "refractive-typographic-lens",
            Self::CornerAnchor => "corner-anchor",
            Self::BarDropCurtain => "bar-drop-curtain",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Bool(bool),
    Text(String),
    Palette(Palette),
    LockStyle(LockStyle),
    Integer(i64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Default,
    User,
    Nix,
    Runtime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Setting {
    pub key: Key,
    pub value: Value,
    pub origin: Origin,
    pub managed: bool,
    pub runtime_override: bool,
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("settings transaction must contain at least one change")]
    EmptyTransaction,
    #[error("unknown setting {0}")]
    UnknownKey(String),
    #[error("invalid type or value for {0}")]
    InvalidValue(&'static str),
    #[error("cannot read settings at {path}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid TOML settings at {path}")]
    Parse {
        path: PathBuf,
        #[source]
        source: toml_edit::TomlError,
    },
    #[error("settings file exceeds 1 MiB: {0}")]
    TooLarge(PathBuf),
    #[error("cannot persist settings at {path}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("writable and managed settings must be different files")]
    SameFile,
    #[error("cannot persist through a symbolic link: {0}")]
    Symlink(PathBuf),
}

/// A successful mutation distinguishes temporary state from disk commitment.
#[derive(Debug)]
pub enum SetOutcome {
    RuntimeOverride,
    Persisted,
    CommittedDurabilityUnconfirmed(DurabilityWarning),
}

#[derive(Debug, Error)]
#[error(
    "settings committed at {path}, but directory synchronization failed; crash durability is unconfirmed"
)]
pub struct DurabilityWarning {
    pub path: PathBuf,
    #[source]
    pub source: std::io::Error,
}

struct Layer {
    document: DocumentMut,
    values: BTreeMap<Key, Value>,
    warnings: Vec<String>,
}

impl Layer {
    fn read(path: &Path, optional: bool) -> Result<Self, Error> {
        let mut bytes = String::new();
        match File::open(path) {
            Ok(file) => {
                file.take(MAX_CONFIG_BYTES + 1)
                    .read_to_string(&mut bytes)
                    .map_err(|source| Error::Read {
                        path: path.to_owned(),
                        source,
                    })?;
                if bytes.len() as u64 > MAX_CONFIG_BYTES {
                    return Err(Error::TooLarge(path.to_owned()));
                }
            }
            Err(error) if optional && error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(Error::Read {
                    path: path.to_owned(),
                    source,
                });
            }
        }
        let document = bytes
            .parse::<DocumentMut>()
            .map_err(|source| Error::Parse {
                path: path.to_owned(),
                source,
            })?;
        let mut values = BTreeMap::new();
        let mut warnings = Vec::new();
        for key in Key::ALL {
            let (tables, field) = key.location();
            let mut table: &dyn toml_edit::TableLike = document.as_table();
            let mut missing = false;
            for name in tables {
                let Some(item) = table.get(name) else {
                    missing = true;
                    break;
                };
                table = item
                    .as_table_like()
                    .ok_or(Error::InvalidValue(key.name()))?;
            }
            if missing {
                continue;
            }
            let Some(item) = table.get(field) else {
                continue;
            };
            // A lock style this version does not know must never stop the locker: it locks in the default.
            if key == Key::LockStyle
                && item
                    .as_str()
                    .is_none_or(|text| LockStyle::parse(text).is_none())
            {
                warnings.push(format!(
                    "{}: unknown lock.style {}; using \"{}\"",
                    path.display(),
                    item.to_string().trim(),
                    LockStyle::DEFAULT.name()
                ));
                continue;
            }
            let candidate = if let Some(text) = item.as_str() {
                key.parse_text(text)?
            } else if let Some(boolean) = item.as_bool() {
                key.parse_bool(boolean)
            } else if let Some(integer) = item.as_integer() {
                Value::Integer(integer)
            } else {
                return Err(Error::InvalidValue(key.name()));
            };
            key.validate(&candidate)?;
            values.insert(key, candidate);
        }
        Ok(Self {
            document,
            values,
            warnings,
        })
    }
}

/// Exactly one owner is retained by the shell process, independently of Luau.
pub struct Owner {
    user_path: PathBuf,
    managed_path: Option<PathBuf>,
    user: Layer,
    managed: BTreeMap<Key, Value>,
    managed_warnings: Vec<String>,
    overrides: BTreeMap<Key, Value>,
}

impl Owner {
    /// Load user and optional managed layers without importing old shell state.
    ///
    /// # Errors
    /// Rejects invalid, oversized, unreadable or aliased configuration files.
    pub fn load(user_path: PathBuf, managed_path: Option<PathBuf>) -> Result<Self, Error> {
        if let Some(managed) = &managed_path {
            if managed == &user_path
                || matches!((fs::canonicalize(managed), fs::canonicalize(&user_path)), (Ok(a), Ok(b)) if a == b)
            {
                return Err(Error::SameFile);
            }
        }
        let user = Layer::read(&user_path, true)?;
        let (managed, managed_warnings) = managed_path
            .as_ref()
            .map(|path| Layer::read(path, false))
            .transpose()?
            .map_or_else(Default::default, |layer| (layer.values, layer.warnings));
        Ok(Self {
            user_path,
            managed_path,
            user,
            managed,
            managed_warnings,
            overrides: BTreeMap::new(),
        })
    }

    /// Values that were ignored on load, each with why; their defaults apply.
    #[must_use]
    pub fn warnings(&self) -> Vec<String> {
        self.managed_warnings
            .iter()
            .chain(&self.user.warnings)
            .cloned()
            .collect()
    }

    #[must_use]
    pub fn managed_path(&self) -> Option<&Path> {
        self.managed_path.as_deref()
    }

    #[must_use]
    pub fn get(&self, key: Key) -> Setting {
        let managed = self.managed.contains_key(&key);
        let (value, origin) = if let Some(value) = self.overrides.get(&key) {
            (value.clone(), Origin::Runtime)
        } else if let Some(value) = self.managed.get(&key) {
            (value.clone(), Origin::Nix)
        } else if let Some(value) = self.user.values.get(&key) {
            (value.clone(), Origin::User)
        } else {
            (key.default_value(), Origin::Default)
        };
        Setting {
            key,
            value,
            origin,
            managed,
            runtime_override: self.overrides.contains_key(&key),
        }
    }

    /// Reload both layers atomically while retaining process overrides.
    ///
    /// # Errors
    /// Leaves the last valid state intact if either layer is invalid.
    pub fn reload(&mut self) -> Result<(), Error> {
        let next = Self::load(self.user_path.clone(), self.managed_path.clone())?;
        self.user = next.user;
        self.managed = next.managed;
        self.managed_warnings = next.managed_warnings;
        Ok(())
    }

    /// Apply a setting, persisting only an unmanaged edit.
    ///
    /// # Errors
    /// Rejects invalid values and pre-commit write failures without changing state.
    /// A post-rename sync failure returns a committed outcome and updates the cache.
    pub fn set(&mut self, key: Key, candidate: Value) -> Result<SetOutcome, Error> {
        self.set_with_sync(key, candidate, |directory| {
            File::open(directory)?.sync_all()
        })
    }

    fn set_with_sync(
        &mut self,
        key: Key,
        candidate: Value,
        sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<SetOutcome, Error> {
        self.set_many_with_sync(BTreeMap::from([(key, candidate)]), sync_directory)
    }

    /// Accept related settings in one disk commit and one cached-state transition.
    ///
    /// # Errors
    /// Pre-commit failure leaves every managed override and writable value intact.
    /// Any managed participant makes the entire related proposal process-local.
    pub fn set_many(&mut self, candidates: BTreeMap<Key, Value>) -> Result<SetOutcome, Error> {
        self.set_many_with_sync(candidates, |directory| File::open(directory)?.sync_all())
    }

    fn set_many_with_sync(
        &mut self,
        candidates: BTreeMap<Key, Value>,
        sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
    ) -> Result<SetOutcome, Error> {
        if candidates.is_empty() {
            return Err(Error::EmptyTransaction);
        }
        for (key, candidate) in &candidates {
            key.validate(candidate)?;
        }
        // A derived operation must not leave only half of its state after restart.
        if candidates
            .keys()
            .any(|key| self.managed.contains_key(key) || self.overrides.contains_key(key))
        {
            self.overrides.extend(candidates);
            return Ok(SetOutcome::RuntimeOverride);
        }
        // Preserve unknown keys and reject concurrent invalid edits before overrides change.
        let mut next = Layer::read(&self.user_path, true)?;
        for (key, candidate) in candidates {
            let layer = &mut next;
            let (tables, field) = key.location();
            let mut table: &mut dyn toml_edit::TableLike = layer.document.as_table_mut();
            for (depth, name) in tables.iter().enumerate() {
                table = table
                    .entry(name)
                    .or_insert_with(|| {
                        let mut new = Table::new();
                        new.set_implicit(depth + 1 < tables.len());
                        Item::Table(new)
                    })
                    .as_table_like_mut()
                    .ok_or(Error::InvalidValue(key.name()))?;
            }
            let item = match &candidate {
                Value::Bool(boolean) => value(*boolean),
                Value::Text(text) => value(text),
                Value::Palette(palette) => value(palette.name()),
                Value::LockStyle(style) => value(style.name()),
                Value::Integer(integer) => value(*integer),
            };
            table.insert(field, item);
            layer.values.insert(key, candidate);
        }
        let serialized = next.document.to_string();
        if serialized.len() as u64 > MAX_CONFIG_BYTES {
            return Err(Error::TooLarge(self.user_path.clone()));
        }
        let outcome = persist(&self.user_path, serialized.as_bytes(), sync_directory)?;
        // Returning from persist means rename committed, even if durability is unknown.
        self.user = next;
        Ok(outcome)
    }
}

fn persist(
    path: &Path,
    bytes: &[u8],
    sync_directory: impl FnOnce(&Path) -> std::io::Result<()>,
) -> Result<SetOutcome, Error> {
    if fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(Error::Symlink(path.to_owned()));
    }
    let directory = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let write = || -> std::io::Result<()> {
        fs::create_dir_all(directory)?;
        // create_new keeps concurrent attempts from sharing a temporary file.
        let temporary = directory.join(format!(".tohru-settings-{}.tmp", std::process::id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        let result = (|| {
            file.write_all(bytes)?;
            file.sync_all()?;
            // Commit point: later failures cannot roll back the new destination.
            fs::rename(&temporary, path)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    };
    write().map_err(|source| Error::Write {
        path: path.to_owned(),
        source,
    })?;
    Ok(match sync_directory(directory) {
        Ok(()) => SetOutcome::Persisted,
        Err(source) => SetOutcome::CommittedDurabilityUnconfirmed(DurabilityWarning {
            path: path.to_owned(),
            source,
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_postcommit_sync_failure_retains_entire_committed_state() {
        let directory =
            std::env::temp_dir().join(format!("tohru-batch-sync-{}", std::process::id()));
        fs::create_dir_all(&directory).unwrap();
        let path = directory.join("config.toml");
        let mut owner = Owner::load(path.clone(), None).unwrap();
        let proposal = BTreeMap::from([
            (Key::Mooncoil, Value::Bool(true)),
            (Key::CornerSize, Value::Integer(48)),
            (Key::Palette, Value::Palette(Palette::Nord)),
        ]);
        let outcome = owner
            .set_many_with_sync(proposal.clone(), |_| {
                Err(std::io::Error::other("injected fsync failure"))
            })
            .unwrap();
        assert!(matches!(
            outcome,
            SetOutcome::CommittedDurabilityUnconfirmed(_)
        ));
        let restarted = Owner::load(path, None).unwrap();
        for (key, value) in proposal {
            assert_eq!(owner.get(key).value, value);
            assert_eq!(owner.get(key), restarted.get(key));
        }
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn postcommit_sync_failure_updates_memory_and_reports_uncertain_durability() {
        let directory =
            std::env::temp_dir().join(format!("tohru-postcommit-{}", std::process::id()));
        fs::create_dir(&directory).unwrap();
        let path = directory.join("config.toml");
        fs::write(&path, "# preserved\n[future]\nvalue = 42\n").unwrap();
        let mut owner = Owner::load(path.clone(), None).unwrap();
        let outcome = owner
            .set_with_sync(Key::Mooncoil, Value::Bool(true), |parent| {
                assert_eq!(parent, directory);
                // Fault occurs after the real file sync/rename, without replacing those effects.
                assert!(
                    fs::read_to_string(&path)
                        .unwrap()
                        .contains("mooncoil = true")
                );
                Err(std::io::Error::other("injected directory fsync failure"))
            })
            .unwrap();
        let SetOutcome::CommittedDurabilityUnconfirmed(warning) = outcome else {
            panic!("committed write must report uncertain crash durability");
        };
        assert_eq!(warning.path, path);
        assert!(warning.to_string().contains("settings committed"));
        assert!(
            warning
                .to_string()
                .contains("crash durability is unconfirmed")
        );
        assert_eq!(
            std::error::Error::source(&warning).unwrap().to_string(),
            "injected directory fsync failure"
        );
        assert_eq!(owner.get(Key::Mooncoil).value, Value::Bool(true));
        assert_eq!(owner.get(Key::Mooncoil).origin, Origin::User);
        let restarted = Owner::load(path.clone(), None).unwrap();
        assert_eq!(owner.get(Key::Mooncoil), restarted.get(Key::Mooncoil));
        assert!(fs::read_to_string(path).unwrap().contains("# preserved"));
        assert_eq!(fs::read_dir(&directory).unwrap().count(), 1);
        assert!(matches!(
            owner.set(Key::Mooncoil, Value::Bool(false)).unwrap(),
            SetOutcome::Persisted
        ));
        fs::remove_dir_all(directory).unwrap();
    }
}

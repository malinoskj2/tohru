use clap::{Args, Parser, Subcommand, ValueEnum};

use super::transfer::Section;

#[derive(Parser, Debug)]
#[command(name = "tohru-assistant", infer_long_args = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum BackendKind {
    Local,
    Orca,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum VoiceChat {
    Home,
    New,
    Foreground,
    Quick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Offline {
    Error,
    Local,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum Switch {
    On,
    Off,
}

impl Switch {
    #[must_use]
    pub fn on(self) -> bool {
        self == Self::On
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum OrcaMode {
    Auto,
    True,
    False,
}

impl OrcaMode {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::True => "true",
            Self::False => "false",
        }
    }
}

#[derive(Args, Clone, Debug, Default)]
pub struct BackendArgs {
    #[arg(long)]
    pub backend: Option<BackendKind>,
    #[arg(long = "orca-bot", default_value = "")]
    pub orca_bot: String,
    #[arg(long = "orca-voice-chat")]
    pub orca_voice_chat: Option<VoiceChat>,
    #[arg(long = "orca-offline")]
    pub orca_offline: Option<Offline>,
}

#[derive(Args, Clone, Debug)]
#[expect(
    clippy::struct_excessive_bools,
    reason = "independent command-line flags"
)]
pub struct ChatArgs {
    #[command(flatten)]
    pub backend: BackendArgs,
    #[arg(long)]
    pub provider: String,
    #[arg(long, default_value = "")]
    pub conversation: String,
    #[arg(long = "new-chat")]
    pub new_chat: bool,
    #[arg(long)]
    pub voice: bool,
    #[arg(long)]
    pub draft: bool,
    /// Read the prompt as one JSON line, then admit nothing until a 'settled' line.
    #[arg(long = "await-settle")]
    pub await_settle: bool,
    #[arg(long, default_value = "")]
    pub interrupted: String,
    /// Strict semantic protocol for tool-free voice drafts.
    #[arg(long)]
    pub semantic: bool,
    #[arg(long = "scenario-model", default_value = "")]
    pub scenario_model: String,
    #[arg(long = "scenario-effort", default_value = "")]
    pub scenario_effort: String,
    #[arg(long, value_enum, default_value_t = Switch::On)]
    pub persona: Switch,
    #[arg(long = "persona-file", default_value = "")]
    pub persona_file: String,
    /// Nonsecret effective settings from the shell.
    #[arg(long = "discord-settings", default_value = "")]
    pub discord_settings: String,
    /// Claim one pending Discord DM; local controller only.
    #[arg(long = "discord-incoming", default_value = "")]
    pub discord_incoming: String,
    #[arg(long, value_enum, default_value_t = OrcaMode::Auto)]
    pub orca: OrcaMode,
    #[arg(long, default_value = "")]
    pub model: String,
    #[arg(long, default_value = "")]
    pub effort: String,
    #[arg(long, value_enum, default_value_t = Switch::On)]
    pub memory: Switch,
    /// Set by quick voice when it hands a question to Home, whose reply it then follows.
    #[arg(skip)]
    pub quick_handoff_notice: String,
    /// The pending settle signal of an `--await-settle` turn.
    #[arg(skip)]
    pub settle_gate: Option<std::sync::Arc<super::chat::SettleGate>>,
}

#[derive(Args, Clone, Debug)]
pub struct RememberArgs {
    #[command(flatten)]
    pub backend: BackendArgs,
    pub conversation: String,
    /// As for providers.
    #[arg(long, default_value = "{}")]
    pub models: String,
    #[arg(long, default_value = "")]
    pub model: String,
    #[arg(long, default_value = "")]
    pub effort: String,
    #[arg(long, value_enum, default_value_t = Switch::On)]
    pub memory: Switch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
pub enum MemoryAction {
    List,
    Forget,
    Clear,
}

#[derive(Subcommand, Debug)]
pub enum NeedsYou {
    Poll {
        #[command(flatten)]
        backend: BackendArgs,
    },
    Wake {
        #[command(flatten)]
        backend: BackendArgs,
        #[arg(required = true, value_name = "UID:EVENT")]
        items: Vec<String>,
    },
}

#[derive(Subcommand, Debug)]
pub enum OrcaCommand {
    Init(InitArgs),
}

#[derive(Args, Debug)]
pub struct InitArgs {
    #[arg(long, default_value = "Tohru")]
    pub bot: String,
    #[arg(long = "persona-file", default_value = "")]
    pub persona_file: String,
    #[arg(long = "no-migrate")]
    pub no_migrate: bool,
    #[arg(long = "force-soul")]
    pub force_soul: bool,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    Providers {
        #[arg(long, default_value = "{}")]
        models: String,
        #[command(flatten)]
        backend: BackendArgs,
    },
    Signin {
        provider: String,
    },
    Chat(Box<ChatArgs>),
    Remember(RememberArgs),
    Commit {
        #[arg(long)]
        provider: String,
        #[arg(long, default_value = "")]
        conversation: String,
        #[arg(long)]
        voice: bool,
        #[arg(long, default_value = "")]
        session: String,
    },
    Rollback {
        conversation: String,
    },
    History {
        #[command(flatten)]
        backend: BackendArgs,
        #[arg(long, default_value = "")]
        query: String,
        #[arg(long, default_value = "")]
        provider: String,
        #[arg(long, allow_negative_numbers = true)]
        since: Option<i64>,
        #[arg(long, default_value_t = 50, allow_negative_numbers = true)]
        limit: i64,
    },
    Show {
        #[command(flatten)]
        backend: BackendArgs,
        conversation: String,
    },
    Delete {
        #[command(flatten)]
        backend: BackendArgs,
        conversation: String,
    },
    Memory {
        #[command(flatten)]
        backend: BackendArgs,
        #[arg(value_enum)]
        action: MemoryAction,
        id: Option<String>,
        #[arg(long)]
        json: bool,
    },
    Sync {
        #[command(flatten)]
        backend: BackendArgs,
    },
    VoiceReady {
        #[arg(long)]
        conversation: String,
        #[arg(long)]
        request: String,
    },
    NeedsYou {
        #[command(subcommand)]
        command: NeedsYou,
    },
    Orca {
        #[command(subcommand)]
        command: OrcaCommand,
    },
    Export {
        file: Option<String>,
    },
    Import {
        file: String,
        #[arg(long, conflicts_with = "replace")]
        merge: bool,
        #[arg(long)]
        replace: bool,
        #[arg(long)]
        yes: bool,
        #[arg(long, value_enum)]
        only: Vec<Section>,
    },
}

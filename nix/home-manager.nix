args@{ config, lib, pkgs, ... }:
let
  cfg = config.programs.tohru;
  sessionHyprland = config.wayland.windowManager.hyprland.package or null;
  # Only a NixOS or nix-darwin integration passes osConfig.
  systemHyprland = args.osConfig.programs.hyprland.package or null;
  hyprland =
    if sessionHyprland != null then sessionHyprland
    else if systemHyprland != null then systemHyprland
    else pkgs.hyprland;
  format = pkgs.formats.toml { };
  assistant = cfg.assistant;
  providers = [ "claude" "codex" "gemini" "agy" "opencode" ];
  efforts = [ "minimal" "low" "medium" "high" "xhigh" "max" ];
  modelSettings = models: lib.mapAttrs (_: m: lib.filterAttrs (_: v: v != null) { inherit (m) model effort; }) models;
  modelSettingsType = lib.types.attrsOf (lib.types.submodule {
    options = {
      model = lib.mkOption { type = lib.types.nullOr lib.types.str; default = null; description = "Model ID passed to the CLI."; };
      effort = lib.mkOption { type = lib.types.nullOr (lib.types.enum efforts); default = null; description = "Reasoning effort passed to the CLI."; };
    };
  });
  profiles = assistant.voiceProfiles;
  setIf = value: attrs: lib.optionalAttrs (value != null) attrs;
  profileSettings = lib.filterAttrs (_: v: v != { }) (
    setIf profiles.vramThresholdPercent { vram_threshold_percent = profiles.vramThresholdPercent; }
    // setIf profiles.hiddenUnloadSeconds { hidden_unload_seconds = profiles.hiddenUnloadSeconds; }
    // setIf profiles.queueTimeoutSeconds { queue_timeout_seconds = profiles.queueTimeoutSeconds; }
    // setIf profiles.modelRoot { model_root = profiles.modelRoot; }
    // {
      low = setIf profiles.low.precision { precision = profiles.low.precision; };
      high = setIf profiles.high.enable { enabled = profiles.high.enable; }
        // setIf profiles.high.precision { precision = profiles.high.precision; }
        // setIf profiles.high.python { python = profiles.high.python; }
        // setIf profiles.high.ttsModel { tts_model = profiles.high.ttsModel; }
        // setIf profiles.high.ttsSpeaker { tts_speaker = profiles.high.ttsSpeaker; }
        // setIf profiles.high.asrModel { asr_model = profiles.high.asrModel; }
        // setIf profiles.high.vramMib { vram_mib = profiles.high.vramMib; };
    });
  assistantSettings = lib.optionalAttrs (!assistant.voice.enable) { voice = false; }
    // lib.optionalAttrs (assistant.tts.loadOnConversation != null) { tts_load_on_conversation = assistant.tts.loadOnConversation; }
    // lib.optionalAttrs (assistant.persona.enable != null) { persona = assistant.persona.enable; }
    // lib.optionalAttrs (assistant.memory.enable != null) { memory = assistant.memory.enable; }
    // lib.optionalAttrs (assistant.orca != null) {
      orca = if builtins.isBool assistant.orca then (if assistant.orca then "true" else "false") else assistant.orca;
    }
    // lib.optionalAttrs (assistant.voice.source != null) { voice_source = assistant.voice.source; }
    // lib.optionalAttrs (assistant.voice.wake != null) { voice_wake = assistant.voice.wake; }
    // lib.optionalAttrs (assistant.voice.vadModel != null) { vad_model = assistant.voice.vadModel; }
    // lib.optionalAttrs (assistant.voice.asrModel != null) { asr_model = assistant.voice.asrModel; }
    // lib.optionalAttrs (assistant.voice.turnModel != null) { turn_model = assistant.voice.turnModel; }
    // lib.optionalAttrs (assistant.voice.kwsModel != null) { kws_model = assistant.voice.kwsModel; }
    // lib.optionalAttrs (assistant.tts.model != null) { tts_model = assistant.tts.model; }
    // lib.optionalAttrs (assistant.provider != null) { provider = assistant.provider; }
    // lib.optionalAttrs (assistant.backend != null) { backend = assistant.backend; }
    // lib.optionalAttrs (assistant.backend == null && assistant.orcaBot.enable) { backend = "orca"; }
    // lib.optionalAttrs (assistant.orcaBot.bot != null) { orca_bot = assistant.orcaBot.bot; }
    // lib.optionalAttrs (assistant.orcaBot.voiceChat != null) { orca_voice_chat = assistant.orcaBot.voiceChat; }
    // lib.optionalAttrs (assistant.orcaBot.offline != null) { orca_offline = assistant.orcaBot.offline; }
    // lib.optionalAttrs (assistant.orcaBot.needsYouMinutes != null) { orca_needs_you_minutes = assistant.orcaBot.needsYouMinutes; }
    // lib.optionalAttrs (assistant.persona.file != null) { persona_file = "${assistant.persona.file}"; }
    // lib.optionalAttrs (assistant.models != { }) {
      models = modelSettings assistant.models;
    }
    // lib.optionalAttrs (assistant.voiceModels != { }) {
      voice_models = modelSettings assistant.voiceModels;
    }
    // lib.optionalAttrs (assistant.semanticModels != { }) {
      semantic_models = modelSettings assistant.semanticModels;
    }
    // lib.optionalAttrs (profileSettings != { }) { voice_profiles = profileSettings; };
  managed = format.generate "tohru-managed.toml"
    (lib.recursiveUpdate cfg.settings (lib.optionalAttrs cfg.polkit.enable { polkit.agent = true; }));
  # hypridle runs outside the login shell, so the locker is told where the managed file is.
  locker = pkgs.writeShellScript "tohru-lock" ''
    TOHRU_MANAGED_CONFIG=${managed} exec ${cfg.package}/bin/tohru-lock "$@"
  '';
  serviceSpecs = builtins.fromJSON (builtins.readFile ../share/tohru/session/services.json);
  service = spec: {
    Unit = {
      Description = spec.description;
      PartOf = [ cfg.systemd.target ];
      After = [ cfg.systemd.target ];
      ConditionEnvironment = "WAYLAND_DISPLAY";
    };
    Service = {
      ExecStart = "${cfg.package}/bin/${spec.program}";
      Environment = [ "TOHRU_MANAGED_CONFIG=${managed}" ];
      Type = spec.type or "simple";
      Restart = "on-failure";
      RestartSec = 2;
      KillMode = spec.kill_mode or "control-group";
      StandardOutput = "journal";
      StandardError = "journal";
    } // lib.optionalAttrs ((spec.type or "") == "notify") {
      NotifyAccess = "all";
      TimeoutStartSec = spec.timeout;
      RestartPreventExitStatus = spec.restart_prevent_exit_status;
    };
  } // lib.optionalAttrs (spec.install or true) {
    Install.WantedBy = [ cfg.systemd.target ];
  };

in {
  options.programs.tohru = {
    enable = lib.mkEnableOption "the Tohru shell package and its managed settings";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./default.nix { withVoice = cfg.assistant.voice.enable; };
      defaultText = lib.literalExpression "tohru with the voice service when programs.tohru.assistant.voice.enable";
    };
    hyprlandPlugin.package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ../hyprland/package.nix { inherit hyprland; };
      defaultText = lib.literalExpression "tohru.packages.\${pkgs.system}.tohru-hyprland";
      description = "Compositor plugin built against this session's Hyprland; nothing loads it automatically.";
    };
    settings = lib.mkOption {
      type = format.type;
      default = { };
      description = "Managed TOML settings; runtime edits remain in memory until process exit.";
    };
    lock.style = lib.mkOption {
      type = lib.types.nullOr (lib.types.enum [
        "still-glass" "ribbon-key" "flooded-monolith" "drifting-frost-veil" "unfolding-glass-capsule"
        "edge-glass-drawer" "horizon-meniscus" "refractive-typographic-lens" "corner-anchor" "bar-drop-curtain"
      ]);
      default = null;
      example = "horizon-meniscus";
      description = ''
        The lock screen's style, written to the managed `lock.style`; `null` leaves
        it to the user's config file (default `flooded-monolith`).
      '';
    };
    notifications.position = lib.mkOption {
      type = lib.types.nullOr (lib.types.enum [ "top-right" "bottom-right" ]);
      default = null;
      example = "bottom-right";
      description = ''
        Where notification toasts stack, written to the managed `notifications.position`;
        `null` leaves it to the user's config file (default `top-right`, under the bar).
      '';
    };
    lock.command = lib.mkOption {
      type = lib.types.str;
      readOnly = true;
      default = "${locker}";
      defaultText = lib.literalExpression "\"<tohru-lock with the managed settings>\"";
      description = ''
        The locker with the managed settings, for hypridle's `lock_cmd`. With
        `--running` it exits 0 only while a locker runs. It needs a PAM service,
        `security.pam.services.tohru-lock = { };` on NixOS.
      '';
    };
    systemd.components = lib.mkOption {
      type = lib.types.listOf (lib.types.enum (builtins.attrNames serviceSpecs));
      default = [ "bar" "session" "launcher" "notifications" ];
      description = "Owned components. Add idle/wallpaper only after explicitly enabling them in settings. Locker is always available on demand.";
    };
    systemd.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Whether to start the bar, session menu, launcher and notification daemon
        as systemd user services with {option}`programs.tohru.systemd.target`;
        the settings window stays on demand. The services inherit the systemd
        user manager's environment, so the compositor session must import
        `WAYLAND_DISPLAY`, and the `PATH` and `XDG_DATA_DIRS` the launcher finds
        and starts applications with, before the target starts (for example with
        UWSM, or `systemctl --user import-environment WAYLAND_DISPLAY PATH XDG_DATA_DIRS`).
        The notification service must be the session's only notification daemon.
      '';
    };
    polkit.enable = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = ''
        Whether Tohru is the session's polkit authentication agent: sets the
        managed `polkit.agent` setting and, with {option}`programs.tohru.systemd.enable`,
        starts the `tohru-polkit` user service. Only one agent can hold a session,
        so disable any other (such as `services.hyprpolkitagent`) first.
      '';
    };
    assistant = {
      voice.enable = lib.mkOption {
        type = lib.types.bool;
        default = true;
        description = ''
          Whether the assistant listens for "Tohru". Disabled, the managed
          `assistant.voice` is false and the default package leaves out the voice
          service and its speech dependencies, so nothing can open the microphone
          or load a model; the chat popup keeps working. Enabled, the setting is
          left to the user's config (on unless they turn it off).
        '';
      };
      voice.echoCancel = lib.mkOption {
        type = lib.types.bool;
        default = false;
        description = ''
          Install a PipeWire `libpipewire-module-echo-cancel` drop-in
          (`~/.config/pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf`) creating a
          WebRTC echo-cancelling source and sink for Tohru without changing the
          session's default audio devices.
        '';
      };
      voice.source = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        description = "Pulse/PipeWire source name; empty chooses automatically. Module metadata reports AEC, never its acoustic effectiveness.";
      };
      voice.wake = lib.mkOption {
        type = lib.types.nullOr (lib.types.enum [ "kws" "asr" ]);
        default = null;
        description = "How the wake word is spotted, written to the managed `assistant.voice_wake`; `null` leaves it to the user's config (`asr`).";
      };
      voice.vadModel = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/srv/models/silero-vad/silero-vad-v6.2.3.onnx";
        description = "The Silero VAD `.onnx` file, written to `assistant.vad_model`. Required for voice; `null` leaves it to the user's config. Nothing is downloaded.";
      };
      voice.asrModel = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/srv/models/sherpa-onnx-streaming-zipformer-en-2023-06-21";
        description = "The sherpa-onnx streaming Zipformer folder (`tokens.txt`, `encoder*.onnx`, `decoder*.onnx`, `joiner*.onnx`), written to `assistant.asr_model`. Required for voice; `null` leaves it to the user's config.";
      };
      voice.turnModel = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/srv/models/smart-turn-v3/smart-turn-v3.2-cpu.onnx";
        description = "The Smart Turn v3.2 `.onnx` file, written to `assistant.turn_model`. Optional: unset, a turn ends after a silence only. `null` leaves it to the user's config.";
      };
      voice.kwsModel = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/srv/models/sherpa-onnx-kws-zipformer-gigaspeech-3.3M-2024-01-01";
        description = "The sherpa-onnx keyword-spotter folder, written to `assistant.kws_model`; only needed with `wake = \"kws\"`. `null` leaves it to the user's config.";
      };
      voiceProfiles = {
        vramThresholdPercent = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 1 100);
          default = null;
          description = "When Tohru appears, the high-VRAM profile loads only if the GPU's committed VRAM (measured use, with running `gpu-run` reservations at their full budget) is below this percentage. Written to `assistant.voice_profiles.vram_threshold_percent`; `null` leaves it to the user's config (50).";
        };
        hiddenUnloadSeconds = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 0 86400);
          default = null;
          description = "How long Tohru stays hidden (no dragon, no popup) before the high-VRAM profile unloads and releases its GPU reservation; 0 unloads at once. Written to `assistant.voice_profiles.hidden_unload_seconds`; `null` leaves it to the user's config (120).";
        };
        queueTimeoutSeconds = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 1 3600);
          default = null;
          description = "How long the high-VRAM profile waits in the GPU scheduler's queue before staying on the low profile. Written to `assistant.voice_profiles.queue_timeout_seconds`; `null` leaves it to the user's config (60).";
        };
        modelRoot = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/mnt/ext-ssd/models";
          description = "The shared model root holding `catalog.json`, `readiness.json` and `by-name/`, written to `assistant.voice_profiles.model_root`. `null` leaves it to the user's config, where empty reads `$MODEL_ROOT`.";
        };
        low.precision = lib.mkOption {
          type = lib.types.nullOr (lib.types.enum [ "int8" "float32" ]);
          default = null;
          description = "Which recogniser and keyword-spotter files the always-loaded CPU profile prefers, written to `assistant.voice_profiles.low.precision`; `null` leaves it to the user's config (`int8`).";
        };
        high.enable = lib.mkOption {
          type = lib.types.nullOr lib.types.bool;
          default = null;
          description = ''
            Let Tohru load the high-VRAM voice profile on the GPU when she appears and
            VRAM use is below the threshold (`assistant.voice_profiles.high.enabled`).
            It runs under a `gpu-run` reservation for as long as its models are
            loaded; the CPU profile stays loaded and answers until it is ready.
            `null` leaves it to the user's config (off).
          '';
        };
        high.precision = lib.mkOption {
          type = lib.types.nullOr (lib.types.enum [ "bfloat16" "float16" "float32" ]);
          default = null;
          description = "Tensor precision of the high-VRAM models, written to `assistant.voice_profiles.high.precision`; `null` leaves it to the user's config (`bfloat16`).";
        };
        high.python = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "/opt/tohru-voice-gpu/bin/python3";
          description = "An interpreter with torch (CUDA), numpy and faster-qwen3-tts (and faster-whisper when `asrModel` is set), written to `assistant.voice_profiles.high.python`. Tohru installs nothing; enabled without it, the high profile is refused with an error.";
        };
        high.ttsModel = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "The `$MODEL_ROOT/by-name` alias of a Qwen3-TTS CustomVoice checkpoint, written to `assistant.voice_profiles.high.tts_model`; `null` leaves it to the user's config (`voice-tts`).";
        };
        high.ttsSpeaker = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "The Qwen3-TTS built-in speaker, written to `assistant.voice_profiles.high.tts_speaker`; `null` leaves it to the user's config (`Serena`).";
        };
        high.asrModel = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          description = "The `$MODEL_ROOT/by-name` alias of a faster-whisper checkpoint that re-transcribes each finished turn on the GPU, written to `assistant.voice_profiles.high.asr_model`. Empty keeps the CPU streaming recogniser; `null` leaves it to the user's config (empty).";
        };
        high.vramMib = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 0 262144);
          default = null;
          description = "The `gpu-run --vram` budget in MiB; 0 estimates it from the catalog's file sizes. Written to `assistant.voice_profiles.high.vram_mib`; `null` leaves it to the user's config (0).";
        };
      };
      tts.loadOnConversation = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = ''
          Load the speech model only for a voice conversation and unload it when
          the conversation ends (`assistant.tts_load_on_conversation`). By default
          it stays loaded (about 0.8 to 1.2 GB) while voice is on, so answers
          speak at once. `null` leaves it to the user's config.
        '';
      };
      tts.model = lib.mkOption {
        type = lib.types.nullOr lib.types.str;
        default = null;
        example = "/srv/models/kokoro-82m";
        description = "The Kokoro folder (`model.onnx`, `tokenizer.json`, `voices/`), written to `assistant.tts_model`. Unset, the dragon speaks with espeak-ng. `null` leaves it to the user's config.";
      };
      provider = lib.mkOption {
        type = lib.types.nullOr (lib.types.enum providers);
        default = null;
        description = "The provider CLI the assistant uses, written to `assistant.provider`; `null` picks the first installed and signed-in one in the order claude, codex, gemini, agy, opencode.";
      };
      backend = lib.mkOption {
        type = lib.types.nullOr (lib.types.enum [ "local" "orca" ]);
        default = null;
        description = "Which backend runs assistant chats (`local` by default, or opt-in `orca` to route through an Orca bot). `null` leaves it to the user's config (or `orca` when `assistant.orcaBot.enable` is true).";
      };
      orcaBot = {
        enable = lib.mkOption {
          type = lib.types.bool;
          default = false;
          description = "Route Tohru's desktop popup and voice through a real Orca bot (`assistant.backend = \"orca\"`). Off by default so non-Orca users have zero Orca dependency.";
        };
        bot = lib.mkOption {
          type = lib.types.nullOr lib.types.str;
          default = null;
          example = "Tohru";
          description = "The Orca bot name Tohru routes through, written to `assistant.orca_bot`; `null` leaves it to the user's config (`Tohru`).";
        };
        voiceChat = lib.mkOption {
          type = lib.types.nullOr (lib.types.enum [ "home" "new" "foreground" "quick" ]);
          default = null;
          description = "Where voice turns land in Orca bot mode: `home` (default) shares Home Chat, `new` starts a fresh conversation, `foreground` retains the experimental separate chat, and opt-in `quick` answers alongside busy Home through a tool-free local responder with exchanges recorded in Home (requires the matching Orca integration patch and Claude CLI). Written to `assistant.orca_voice_chat`.";
        };
        offline = lib.mkOption {
          type = lib.types.nullOr (lib.types.enum [ "error" "local" ]);
          default = null;
          description = "Behavior when `assistant.backend = \"orca\"` and Orca is unreachable (`error` to prevent split-brain, or `local` to fall back), written to `assistant.orca_offline`.";
        };
        needsYouMinutes = lib.mkOption {
          type = lib.types.nullOr (lib.types.ints.between 0 1440);
          default = null;
          example = 10;
          description = "Minutes between the bar's checks for the bot's items that newly need you, each announced once on the desktop and aloud, written to `assistant.orca_needs_you_minutes`; `0` turns the watcher off and `null` leaves it to the user's config (5).";
        };
      };
      models = lib.mkOption {
        type = modelSettingsType;
        default = { };
        example = lib.literalExpression ''{ codex = { model = "gpt-6.2-sol"; effort = "high"; }; }'';
        description = "Per-provider model and effort, written to `assistant.models.<provider>`; unset ones use Tohru's defaults (see docs/ai-assistant.md).";
      };
      voiceModels = lib.mkOption {
        type = modelSettingsType;
        default = { };
        description = "Per-provider model and effort overrides for tool-free spoken drafts (assistant.voice_models).";
      };
      semanticModels = lib.mkOption {
        type = modelSettingsType;
        default = { };
        description = "Per-provider overrides for semantic decisions and draft answers (assistant.semantic_models); does not enable the feature.";
      };
      persona.enable = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether answers come from Tohru's persona (`assistant.persona`, default on); off sends no system prompt at all. `null` leaves it to the user's config.";
      };
      persona.file = lib.mkOption {
        type = lib.types.nullOr lib.types.path;
        default = null;
        description = "A persona file used instead of the shipped one (`assistant.persona_file`).";
      };
      memory.enable = lib.mkOption {
        type = lib.types.nullOr lib.types.bool;
        default = null;
        description = "Whether Tohru remembers facts and summaries between chats (`assistant.memory`, default on); off keeps history only. `null` leaves it to the user's config.";
      };
      orca = lib.mkOption {
        type = lib.types.nullOr (lib.types.oneOf [ (lib.types.enum [ "auto" "true" "false" ]) lib.types.bool ]);
        default = null;
        description = ''
          Whether the assistant gets Orca CLI context (`assistant.orca`, default `"auto"`):
          `"auto"` detects Orca when installed, `"true"` always enables it, and `"false"`
          disables detection, environment variables and prompt context. `null` leaves it
          to the user's config.
        '';
      };
    };
    systemd.target = lib.mkOption {
      type = lib.types.str;
      default = "graphical-session.target";
      description = "The systemd user target that starts and stops the Tohru services.";
    };
  };
  config = lib.mkIf cfg.enable (lib.mkMerge [
    {
      home.packages = [ cfg.package ];
      home.sessionVariables.TOHRU_MANAGED_CONFIG = toString managed;
    }
    (lib.mkIf (cfg.lock.style != null) {
      programs.tohru.settings.lock.style = cfg.lock.style;
    })
    (lib.mkIf (assistantSettings != { }) { programs.tohru.settings.assistant = assistantSettings; })
    (lib.mkIf (assistant.voice.enable && assistant.voice.echoCancel) {
      xdg.configFile."pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf".source =
        ../share/tohru/pipewire/60-tohru-echo-cancel.conf;
    })
    {
      assertions = [{
        assertion = builtins.all (name: builtins.elem name providers) (builtins.attrNames assistant.models);
        message = "programs.tohru.assistant.models takes only ${lib.concatStringsSep ", " providers}";
      }];
    }
    (lib.mkIf (cfg.notifications.position != null) {
      programs.tohru.settings.notifications.position = cfg.notifications.position;
    })
    (lib.mkIf cfg.systemd.enable {
      systemd.user.services = lib.mapAttrs' (name: spec:
        lib.nameValuePair "tohru-${name}" (service spec))
        (lib.filterAttrs (name: _: builtins.elem name cfg.systemd.components || name == "lock"
          || (name == "polkit" && cfg.polkit.enable)) serviceSpecs);
    })
  ]);
}

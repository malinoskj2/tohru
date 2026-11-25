# Evaluate with a real Home Manager flake/source and caller-selected package set.
{ pkgs, homeManager, tohru }:
let
  base = {
    home.username = "tohru-check";
    home.homeDirectory = "/home/tohru-check";
    home.stateVersion = "26.05";
  };
  settings = {
    bar.mooncoil = true;
    bar.preferred_output = "DP-CHECK";
    appearance.corners = false;
  };
  home = module: modules: homeManager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [ module base ] ++ modules;
  };
  enabled = home tohru.homeManagerModules.tohru [ { programs.tohru = { enable = true; inherit settings; }; } ];
  config = enabled.config;
  disabled = (home tohru.homeManagerModules.default [ ]).config;
  serviced = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; inherit settings; systemd.enable = true; };
  } ]).config;
  retargeted = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; systemd.enable = true; systemd.target = "hyprland-session.target"; };
  } ]).config;
  agent = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; inherit settings; systemd.enable = true; polkit.enable = true; notifications.position = "bottom-right"; };
  } ]).config;
  portal = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; systemd.enable = true; systemd.components = [ "shortcuts" ];
      settings.shortcuts = { mode = "portal"; launcher_trigger = "<Super>space"; };
    };
  } ]).config;
  agentManaged = builtins.readFile agent.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  quiet = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = {
      enable = true;
      assistant = {
        voice.enable = false;
        memory.enable = false;
        orca = false;
        voice.wake = "kws";
        voice.kwsModel = "/k";
        voice.vadModel = "/vad.onnx";
        voice.asrModel = "/asr";
        voice.turnModel = "/turn.onnx";
        tts.model = "/kokoro";
        provider = "codex";
        models.codex = { model = "gpt-6.2-sol"; effort = "high"; };
        models.claude.effort = "low";
        voiceModels.codex = { model = "gpt-6-luna"; effort = "low"; };
        semanticModels.codex = { model = "gpt-6.2-sol"; effort = "medium"; };
      };
    };
  } ]).config;
  quietManaged = builtins.readFile quiet.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  noEchoCancel = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; assistant.voice.echoCancel = false; };
  } ]).config;
  echoEnabled = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = { enable = true; assistant.voice.echoCancel = true; };
  } ]).config;
  echoCancelConf = builtins.readFile echoEnabled.xdg.configFile."pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf".source;
  orcaBot = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = {
      enable = true;
      assistant.voice.source = "usb-integration-mic";
      assistant.orca = "auto";
      assistant.orcaBot = {
        enable = true;
        bot = "Tohru";
        voiceChat = "home";
        offline = "error";
      };
    };
  } ]).config;
  orcaManaged = builtins.readFile orcaBot.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  foregroundBot = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = {
      enable = true;
      assistant.orcaBot = { enable = true; voiceChat = "foreground"; };
    };
  } ]).config;
  profiled = (home tohru.homeManagerModules.tohru [ {
    programs.tohru = {
      enable = true;
      assistant.voiceProfiles = {
        vramThresholdPercent = 40;
        hiddenUnloadSeconds = 0;
        modelRoot = "/models";
        low.precision = "float32";
        high = { enable = true; python = "/gpu/bin/python3"; asrModel = "whisper-large-v3-turbo"; vramMib = 9000; };
      };
    };
  } ]).config;
  profiledManaged = builtins.readFile profiled.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  servicesOnly = (home tohru.homeManagerModules.tohru [ { programs.tohru.systemd.enable = true; } ]).config;
  roles = {
    tohru-bar = "tohru-run";
    tohru-session = "tohru-session-run";
    tohru-launcher = "tohru-launcher-run";
    tohru-notifications = "tohru-notifications-run";
    tohru-lock = "tohru-lock";
  };
  managed = serviced.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  targets = cfg: target: builtins.all (name:
    let unit = cfg.systemd.user.services.${name}; in
    unit.Unit.PartOf == [ target ] && unit.Unit.After == [ target ]
    && (name == "tohru-lock" || unit.Install.WantedBy == [ target ])) (builtins.attrNames roles);
  units = builtins.mapAttrs (name: _: builtins.readFile serviced.xdg.configFile."systemd/user/${name}.service".source) roles;
in
assert builtins.all (entry: entry.assertion) config.assertions;
assert builtins.all (entry: entry.assertion) disabled.assertions;
assert builtins.all (entry: entry.assertion) serviced.assertions;
assert builtins.elem config.programs.tohru.package config.home.packages;
assert !config.programs.tohru.systemd.enable;
assert config.programs.tohru.systemd.target == "graphical-session.target";
assert config.systemd.user.services == { };
assert !(disabled.home.sessionVariables ? TOHRU_MANAGED_CONFIG);
assert disabled.systemd.user.services == { };
assert servicesOnly.systemd.user.services == { };
assert builtins.attrNames serviced.systemd.user.services == builtins.attrNames roles;
assert builtins.all (name:
  serviced.systemd.user.services.${name}.Service.ExecStart
    == [ "${serviced.programs.tohru.package}/bin/${roles.${name}}" ]
  && serviced.systemd.user.services.${name}.Service.Restart == "on-failure")
  (builtins.attrNames roles);
assert targets serviced "graphical-session.target";
assert targets retargeted "hyprland-session.target";
assert builtins.all (name: serviced.systemd.user.services.${name}.Service.Environment
  == [ "TOHRU_MANAGED_CONFIG=${managed}" ]) (builtins.attrNames roles);
assert !(pkgs.lib.hasInfix "agent" (builtins.readFile managed));
assert !(pkgs.lib.hasInfix "notifications" (builtins.readFile managed));
assert pkgs.lib.hasInfix "[notifications]\nposition = \"bottom-right\"" agentManaged;
assert builtins.attrNames agent.systemd.user.services == builtins.attrNames (roles // { tohru-polkit = ""; });
assert agent.systemd.user.services.tohru-polkit.Service.ExecStart == [ "${agent.programs.tohru.package}/bin/tohru-polkit-run" ];
assert agent.systemd.user.services.tohru-polkit.Unit.PartOf == [ "graphical-session.target" ];
assert pkgs.lib.hasInfix "[polkit]\nagent = true" agentManaged;
assert pkgs.lib.hasInfix "preferred_output = \"DP-CHECK\"" agentManaged;
assert !(pkgs.lib.hasInfix "assistant" (builtins.readFile managed));
assert portal.systemd.user.services.tohru-shortcuts.Service.ExecStart == [ "${portal.programs.tohru.package}/bin/tohru-shortcuts" ];
assert portal.programs.tohru.settings.shortcuts.mode == "portal";
assert config.programs.tohru.package.passthru.withVoice;
assert !quiet.programs.tohru.package.passthru.withVoice;
assert !config.programs.tohru.assistant.voice.echoCancel;
assert echoEnabled.programs.tohru.assistant.voice.echoCancel;
assert !(config.xdg.configFile ? "pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf");
assert echoEnabled.xdg.configFile ? "pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf";
assert pkgs.lib.hasInfix "libpipewire-module-echo-cancel" echoCancelConf;
assert pkgs.lib.hasInfix "aec/libspa-aec-webrtc" echoCancelConf;
assert pkgs.lib.hasInfix "tohru-echo-cancel-source" echoCancelConf;
assert pkgs.lib.hasInfix "tohru-echo-cancel-sink" echoCancelConf;
assert pkgs.lib.hasInfix "priority.session = 0" echoCancelConf;
assert !noEchoCancel.programs.tohru.assistant.voice.echoCancel;
assert !(noEchoCancel.xdg.configFile ? "pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf");
assert !quiet.programs.tohru.assistant.voice.echoCancel;
assert !(quiet.xdg.configFile ? "pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf");
assert !(disabled.xdg.configFile ? "pipewire/pipewire.conf.d/60-tohru-echo-cancel.conf");
assert pkgs.lib.hasInfix "[assistant]\nasr_model = \"/asr\"\nkws_model = \"/k\"\nmemory = false\norca = \"false\"\nprovider = \"codex\"\ntts_model = \"/kokoro\"\nturn_model = \"/turn.onnx\"\nvad_model = \"/vad.onnx\"\nvoice = false\nvoice_wake = \"kws\"" quietManaged;
assert pkgs.lib.hasInfix "[assistant.voice_models.codex]\neffort = \"low\"\nmodel = \"gpt-6-luna\"" quietManaged;
assert pkgs.lib.hasInfix "[assistant.semantic_models.codex]\neffort = \"medium\"\nmodel = \"gpt-6.2-sol\"" quietManaged;
assert pkgs.lib.hasInfix "[assistant.models.codex]\neffort = \"high\"\nmodel = \"gpt-6.2-sol\"" quietManaged;
assert pkgs.lib.hasInfix "[assistant.models.claude]\neffort = \"low\"" quietManaged;
assert pkgs.lib.hasInfix "[assistant]\nbackend = \"orca\"\norca = \"auto\"\norca_bot = \"Tohru\"\norca_offline = \"error\"\norca_voice_chat = \"home\"\nvoice_source = \"usb-integration-mic\"" orcaManaged;
assert pkgs.lib.hasInfix "orca_voice_chat = \"foreground\"" (builtins.readFile foregroundBot.home.sessionVariables.TOHRU_MANAGED_CONFIG);
assert !(pkgs.lib.hasInfix "orca_bot" quietManaged) && !(pkgs.lib.hasInfix "backend =" quietManaged);
assert pkgs.lib.hasInfix "[assistant.voice_profiles]\nhidden_unload_seconds = 0\nmodel_root = \"/models\"\nvram_threshold_percent = 40" profiledManaged;
assert pkgs.lib.hasInfix "[assistant.voice_profiles.high]\nasr_model = \"whisper-large-v3-turbo\"\nenabled = true\npython = \"/gpu/bin/python3\"\nvram_mib = 9000" profiledManaged;
assert pkgs.lib.hasInfix "[assistant.voice_profiles.low]\nprecision = \"float32\"" profiledManaged;
assert !(pkgs.lib.hasInfix "voice_profiles" quietManaged) && !(pkgs.lib.hasInfix "voice_profiles" orcaManaged);
assert serviced.systemd.user.services.tohru-launcher.Service.KillMode == "process";
assert serviced.systemd.user.services.tohru-lock.Service.Type == "notify";
assert builtins.all (name: serviced.systemd.user.services.${name}.Service.KillMode == "control-group")
  [ "tohru-bar" "tohru-session" "tohru-notifications" ];
{
  activation = enabled.activationPackage.drvPath;
  managed = config.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  packages = map (package: package.name) config.home.packages;
  services = builtins.attrNames serviced.systemd.user.services;
  inherit units;
  assertions = true;
}

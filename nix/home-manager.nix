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
  managed = format.generate "tohru-managed.toml" cfg.settings;
  service = description: program: extra: lib.recursiveUpdate {
    Unit = {
      Description = description;
      PartOf = [ cfg.systemd.target ];
      After = [ cfg.systemd.target ];
      ConditionEnvironment = "WAYLAND_DISPLAY";
    };
    Service = {
      ExecStart = "${cfg.package}/bin/${program}";
      Environment = [ "TOHRU_MANAGED_CONFIG=${managed}" ];
      Restart = "on-failure";
      RestartSec = 2;
    };
    Install.WantedBy = [ cfg.systemd.target ];
  } extra;
in {
  options.programs.tohru = {
    enable = lib.mkEnableOption "the Tohru shell package and its managed settings";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./default.nix { };
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
    (lib.mkIf cfg.systemd.enable {
      systemd.user.services = {
        tohru-bar = service "Tohru bar" "tohru-run" { };
        tohru-session = service "Tohru session menu" "tohru-session-run" { };
        # Launched applications stay in this unit's cgroup; restarting the launcher must not kill them.
        tohru-launcher = service "Tohru application launcher" "tohru-launcher-run" {
          Service.KillMode = "process";
        };
        tohru-notifications = service "Tohru notification daemon" "tohru-notifications-run" { };
      };
    })
  ]);
}

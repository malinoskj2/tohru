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
  managed = format.generate "tohru-managed.toml"
    (lib.recursiveUpdate cfg.settings (lib.optionalAttrs cfg.polkit.enable { polkit.agent = true; }));
  # hypridle runs outside the login shell, so the locker is told where the managed file is.
  locker = pkgs.writeShellScript "tohru-lock" ''
    TOHRU_MANAGED_CONFIG=${managed} exec ${cfg.package}/bin/tohru-lock "$@"
  '';
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
    (lib.mkIf cfg.systemd.enable {
      systemd.user.services = {
        tohru-bar = service "Tohru bar" "tohru-run" { };
        tohru-session = service "Tohru session menu" "tohru-session-run" { };
        # Launched applications stay in this unit's cgroup; restarting the launcher must not kill them.
        tohru-launcher = service "Tohru application launcher" "tohru-launcher-run" {
          Service.KillMode = "process";
        };
        tohru-notifications = service "Tohru notification daemon" "tohru-notifications-run" { };
      } // lib.optionalAttrs cfg.polkit.enable {
        tohru-polkit = service "Tohru polkit authentication agent" "tohru-polkit-run" { };
      };
    })
  ]);
}

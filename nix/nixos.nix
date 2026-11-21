{ config, lib, pkgs, ... }:
let
  cfg = config.services.tohru-greeter;
  format = pkgs.formats.toml { };

  managedConfig = format.generate "tohru-greeter.toml" (lib.recursiveUpdate cfg.settings {
    greeter = lib.filterAttrs (_: v: v != null) {
      style = cfg.style;
      wallpaper = if cfg.wallpaper != null then toString cfg.wallpaper else null;
      preferred_output = cfg.preferredOutput;
      default_user = cfg.defaultUser;
      default_session = cfg.defaultSession;
      allow_power_actions = cfg.allowPowerActions;
      hidden_users = config.services.displayManager.hiddenUsers ++ [ "greeter" ];
      avatars = lib.mapAttrs (_: toString) cfg.avatars;
    };
  });

  sessionsDir = "${config.services.displayManager.sessionData.desktops}/share";

  greeterRun = pkgs.writeShellScript "tohru-greeter-run" ''
    export TOHRU_SHELL=1
    export TOHRU_GREETER=1
    export PLEAMAR_NO_PORTAL=1
    export PLEAMAR_GENERIC=1
    export TOHRU_MANAGED_CONFIG=${managedConfig}
    export TOHRU_GREETER_SESSIONS_DIRS=${lib.escapeShellArg sessionsDir}
    export TOHRU_GREETER_STATE_DIR=/var/lib/tohru-greeter
    export XDG_CACHE_HOME="''${XDG_RUNTIME_DIR:-/tmp}/tohru-greeter-cache"
    mkdir -p "$XDG_CACHE_HOME"
    exec ${cfg.package}/bin/tohru-greeter
  '';

  hyprlandConf = pkgs.writeText "tohru-greeter-hyprland.lua" ''
    hl.config(${lib.generators.toLua { } {
      misc = {
        disable_hyprland_logo = true;
        disable_splash_rendering = true;
        force_default_wallpaper = 0;
      };
      animations.enabled = false;
      input = {
        kb_layout = cfg.keyboard.layout;
        kb_variant = cfg.keyboard.variant;
        kb_options = cfg.keyboard.options;
        repeat_delay = cfg.keyboard.repeatDelay;
        repeat_rate = cfg.keyboard.repeatRate;
      };
    }})
    ${lib.concatMapStringsSep "\n" (m: "hl.monitor(${lib.generators.toLua { } {
      inherit (m) output mode position scale transform;
    }})") cfg.monitors}
    ${cfg.extraCompositorConfig}
    hl.on("hyprland.start", function()
      hl.exec_cmd(${lib.generators.toLua { } "${greeterRun}; ${lib.getExe' cfg.hyprlandPackage "hyprctl"} dispatch 'hl.dsp.exit()'"})
    end)
  '';

  swayConf = pkgs.writeText "tohru-greeter-sway.conf" ''
    ${lib.concatMapStringsSep "\n" (m:
      "output ${m.output} mode ${m.mode} pos ${lib.replaceStrings [ "x" ] [ " " ] m.position} scale ${toString m.scale} transform ${toString (m.transform * 90)}"
    ) cfg.monitors}
    input type:keyboard {
      xkb_layout ${cfg.keyboard.layout}
      xkb_variant "${cfg.keyboard.variant}"
      xkb_options "${cfg.keyboard.options}"
      repeat_delay ${toString cfg.keyboard.repeatDelay}
      repeat_rate ${toString cfg.keyboard.repeatRate}
    }
    ${cfg.extraCompositorConfig}
    exec "${greeterRun}; ${lib.getExe' cfg.swayPackage "swaymsg"} exit"
  '';

  tuiFallback = lib.optionalString cfg.fallbackToTui ''
    if [ "$status" -ne 0 ]; then
      exec ${lib.getExe cfg.fallbackTuiPackage} --time --sessions ${lib.escapeShellArg "${sessionsDir}/wayland-sessions"}
    fi
  '';

  compositorCmd =
    if cfg.compositor == "hyprland" then
      pkgs.writeShellScript "tohru-greeter-session" ''
        export XDG_CONFIG_HOME="''${XDG_RUNTIME_DIR:-/tmp}/tohru-greeter-xdg"
        mkdir -p "$XDG_CONFIG_HOME"
        ${lib.getExe cfg.hyprlandPackage} -c ${hyprlandConf}
        status=$?
        ${tuiFallback}
        exit "$status"
      ''
    else
      pkgs.writeShellScript "tohru-greeter-session" ''
        ${lib.getExe cfg.swayPackage} ${lib.optionalString cfg.swayUnsupportedGpu "--unsupported-gpu"} --config ${swayConf}
        status=$?
        ${tuiFallback}
        exit "$status"
      '';
in {
  options.services.tohru-greeter = {
    enable = lib.mkEnableOption "the Tohru Wayland login screen via greetd";

    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./default.nix { };
      defaultText = lib.literalExpression "tohru.packages.\${pkgs.system}.default";
      description = "Tohru shell package providing `tohru-greeter`.";
    };

    compositor = lib.mkOption {
      type = lib.types.enum [ "hyprland" "sway" ];
      default = if (config.programs.hyprland.enable or false) then "hyprland" else "sway";
      defaultText = lib.literalExpression ''if config.programs.hyprland.enable then "hyprland" else "sway"'';
      description = "Wayland compositor used to host `tohru-greeter` inside greetd.";
    };

    hyprlandPackage = lib.mkOption {
      type = lib.types.package;
      default = config.programs.hyprland.package or pkgs.hyprland;
      defaultText = lib.literalExpression "config.programs.hyprland.package or pkgs.hyprland";
      description = "Hyprland package used when `compositor = \"hyprland\"`.";
    };

    swayPackage = lib.mkOption {
      type = lib.types.package;
      default = pkgs.sway;
      defaultText = lib.literalExpression "pkgs.sway";
      description = "Sway package used when `compositor = \"sway\"`.";
    };

    swayUnsupportedGpu = lib.mkOption {
      type = lib.types.bool;
      default = lib.elem "nvidia" (config.services.xserver.videoDrivers or [ ]);
      defaultText = lib.literalExpression ''lib.elem "nvidia" config.services.xserver.videoDrivers'';
      description = "Pass `--unsupported-gpu` to Sway (required on NVIDIA drivers).";
    };

    style = lib.mkOption {
      type = lib.types.enum [
        "still-glass" "ribbon-key" "flooded-monolith" "drifting-frost-veil"
        "unfolding-glass-capsule" "edge-glass-drawer" "horizon-meniscus"
        "refractive-typographic-lens" "corner-anchor" "bar-drop-curtain" "plain"
      ];
      default = "flooded-monolith";
      description = "Visual style of the login screen (shared with `tohru-lock`).";
    };

    wallpaper = lib.mkOption {
      type = lib.types.nullOr lib.types.path;
      default = null;
      description = "Optional static image (PNG/JPEG/WebP/SVG) copied to the Nix store; `null` uses the dusk gradient.";
    };

    avatars = lib.mkOption {
      type = lib.types.attrsOf lib.types.path;
      default = { };
      example = lib.literalExpression "{ jesse = ./avatar.png; }";
      description = "Optional per-user avatar images copied to the Nix store (also linked into AccountsService icons).";
    };

    preferredOutput = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "DP-2";
      description = "Preferred output name for the primary interactive login surface; falls back to the first connected output if disconnected.";
    };

    defaultUser = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      description = "Username to pre-select when no last login state exists yet.";
    };

    defaultSession = lib.mkOption {
      type = lib.types.nullOr lib.types.str;
      default = null;
      example = "hyprland-uwsm";
      description = "Session desktop stem (without `.desktop`) to pre-select when no last login state exists yet.";
    };

    monitors = lib.mkOption {
      type = lib.types.listOf (lib.types.submodule {
        options = {
          output = lib.mkOption { type = lib.types.str; };
          mode = lib.mkOption { type = lib.types.str; default = "preferred"; };
          position = lib.mkOption { type = lib.types.str; default = "auto"; };
          scale = lib.mkOption { type = lib.types.number; default = 1; };
          transform = lib.mkOption { type = lib.types.ints.between 0 7; default = 0; };
        };
      });
      default = [ ];
      description = "Monitor modes, positions, scales and transforms applied in the greeter compositor.";
    };

    keyboard = {
      layout = lib.mkOption {
        type = lib.types.str;
        default = config.services.xserver.xkb.layout or "us";
        defaultText = lib.literalExpression ''config.services.xserver.xkb.layout or "us"'';
      };
      variant = lib.mkOption {
        type = lib.types.str;
        default = config.services.xserver.xkb.variant or "";
        defaultText = lib.literalExpression ''config.services.xserver.xkb.variant or ""'';
      };
      options = lib.mkOption {
        type = lib.types.str;
        default = config.services.xserver.xkb.options or "";
        defaultText = lib.literalExpression ''config.services.xserver.xkb.options or ""'';
      };
      repeatDelay = lib.mkOption { type = lib.types.ints.positive; default = 260; };
      repeatRate = lib.mkOption { type = lib.types.ints.positive; default = 24; };
    };

    allowPowerActions = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Show Sleep, Restart and Shut Down controls and install the scoped Polkit rule for the `greeter` user.";
    };

    allowMultipleSessionPower = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Also permit the `greeter` user to suspend/reboot/poweroff while another user session is active.";
    };

    fallbackToTui = lib.mkOption {
      type = lib.types.bool;
      default = true;
      description = "Fall back to `tuigreet` on VT1 if the greeter compositor exits with an error.";
    };

    fallbackTuiPackage = lib.mkOption {
      type = lib.types.package;
      default = pkgs.tuigreet;
      defaultText = lib.literalExpression "pkgs.tuigreet";
      description = "TUI greeter package used when `fallbackToTui` triggers.";
    };

    extraCompositorConfig = lib.mkOption {
      type = lib.types.lines;
      default = "";
      description = ''
        Additional raw lines appended to the greeter compositor config: Lua (Hyprland 0.57+
        `hl.*` calls) for `compositor = "hyprland"`, Sway config syntax for `compositor = "sway"`.
      '';
    };

    settings = lib.mkOption {
      type = format.type;
      default = { };
      description = "Additional TOML keys merged into the read-only greeter configuration file.";
    };
  };

  config = lib.mkIf cfg.enable {
    assertions = [{
      assertion = cfg.compositor != "hyprland" || lib.versionAtLeast (lib.getVersion cfg.hyprlandPackage) "0.57";
      message = "services.tohru-greeter writes a Lua Hyprland config, which needs Hyprland 0.57 or later.";
    }];

    services.greetd = {
      enable = true;
      restart = lib.mkDefault true;
      settings.default_session = {
        user = "greeter";
        command = toString compositorCmd;
      };
    };

    users.users.greeter.extraGroups = [ "video" "render" ];

    # AccountsService allows users to set avatars imperatively (`tohru avatar set`) across 0700 home dirs.
    services.accounts-daemon.enable = lib.mkDefault true;

    systemd.tmpfiles.rules = [
      "d /var/lib/tohru-greeter 0750 greeter greeter -"
    ] ++ lib.mapAttrsToList (user: path:
      "L+ /var/lib/AccountsService/icons/${user} - - - - ${path}"
    ) cfg.avatars;

    security.polkit.enable = lib.mkIf cfg.allowPowerActions true;
    security.polkit.extraConfig = lib.mkIf cfg.allowPowerActions ''
      polkit.addRule(function(action, subject) {
        var actions = [
          "org.freedesktop.login1.suspend",
          "org.freedesktop.login1.reboot",
          "org.freedesktop.login1.power-off"
          ${lib.optionalString cfg.allowMultipleSessionPower '',
          "org.freedesktop.login1.suspend-multiple-sessions",
          "org.freedesktop.login1.reboot-multiple-sessions",
          "org.freedesktop.login1.power-off-multiple-sessions"''}
        ];
        if (subject.user == "greeter" && subject.local && subject.active && actions.indexOf(action.id) >= 0) {
          return polkit.Result.YES;
        }
      });
    '';
  };
}

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
in {
  options.programs.tohru = {
    enable = lib.mkEnableOption "the Tohru shell package (no automatic service activation)";
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
  };
  config = lib.mkIf cfg.enable {
    home.packages = [ cfg.package ];
    home.sessionVariables.TOHRU_MANAGED_CONFIG = toString managed;
  };
}

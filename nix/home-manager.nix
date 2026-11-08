{ config, lib, pkgs, ... }:
let
  cfg = config.programs.tohru;
  format = pkgs.formats.toml { };
  managed = format.generate "tohru-managed.toml" cfg.settings;
in {
  options.programs.tohru = {
    enable = lib.mkEnableOption "the Tohru shell package (no automatic service activation)";
    package = lib.mkOption {
      type = lib.types.package;
      default = pkgs.callPackage ./default.nix { };
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

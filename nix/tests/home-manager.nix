# Evaluate with a real Home Manager flake/source and caller-selected package set.
{ pkgs, homeManager, tohru }:
let
  base = {
    home.username = "tohru-check";
    home.homeDirectory = "/home/tohru-check";
    home.stateVersion = "26.05";
  };
  enabled = homeManager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [ tohru.homeManagerModules.tohru base {
      programs.tohru = {
        enable = true;
        settings = {
          bar.mooncoil = true;
          bar.preferred_output = "DP-CHECK";
          appearance.corners = false;
        };
      };
    } ];
  };
  disabled = homeManager.lib.homeManagerConfiguration {
    inherit pkgs;
    modules = [ tohru.homeManagerModules.default base ];
  };
  config = enabled.config;
in
assert builtins.all (entry: entry.assertion) config.assertions;
assert builtins.all (entry: entry.assertion) disabled.config.assertions;
assert builtins.elem config.programs.tohru.package config.home.packages;
assert config.systemd.user.services == { };
assert !(disabled.config.home.sessionVariables ? TOHRU_MANAGED_CONFIG);
{
  activation = enabled.activationPackage.drvPath;
  managed = config.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  packages = map (package: package.name) config.home.packages;
  services = builtins.attrNames config.systemd.user.services;
  assertions = true;
}

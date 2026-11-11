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
  servicesOnly = (home tohru.homeManagerModules.tohru [ { programs.tohru.systemd.enable = true; } ]).config;
  roles = {
    tohru-bar = "tohru-run";
    tohru-session = "tohru-session-run";
    tohru-launcher = "tohru-launcher-run";
    tohru-notifications = "tohru-notifications-run";
  };
  managed = serviced.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  targets = cfg: target: builtins.all (name:
    let unit = cfg.systemd.user.services.${name}; in
    unit.Unit.PartOf == [ target ] && unit.Unit.After == [ target ]
    && unit.Install.WantedBy == [ target ]) (builtins.attrNames roles);
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
assert serviced.systemd.user.services.tohru-bar.Service.Environment == [ "TOHRU_MANAGED_CONFIG=${managed}" ];
assert serviced.systemd.user.services.tohru-launcher.Service.KillMode == "process";
assert builtins.all (name: !(serviced.systemd.user.services.${name}.Service ? KillMode))
  [ "tohru-bar" "tohru-session" "tohru-notifications" ];
{
  activation = enabled.activationPackage.drvPath;
  managed = config.home.sessionVariables.TOHRU_MANAGED_CONFIG;
  packages = map (package: package.name) config.home.packages;
  services = builtins.attrNames serviced.systemd.user.services;
  inherit units;
  assertions = true;
}

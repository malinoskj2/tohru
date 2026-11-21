# Evaluate the NixOS greeter module against a real nixpkgs NixOS evaluation.
{ nixpkgs, tohru, system ? "x86_64-linux" }:
let
  lib = nixpkgs.lib;
  eval = extraModules: (nixpkgs.lib.nixosSystem {
    inherit system;
    modules = [
      tohru.nixosModules.tohru-greeter
      ({ pkgs, ... }: {
        boot.loader.grub.enable = false;
        fileSystems."/".device = "/dev/null";
        system.stateVersion = "26.05";
        # Avoid building the full Rust derivation during pure module evaluation checks.
        services.tohru-greeter.package = pkgs.emptyDirectory // { meta.mainProgram = "tohru"; };
        services.tohru-greeter.hyprlandPackage = pkgs.emptyDirectory // { meta.mainProgram = "Hyprland"; };
        services.tohru-greeter.swayPackage = pkgs.emptyDirectory // { meta.mainProgram = "sway"; };
        services.tohru-greeter.fallbackTuiPackage = pkgs.emptyDirectory // { meta.mainProgram = "tuigreet"; };
      })
    ] ++ extraModules;
  }).config;

  disabled = eval [ ];

  hyprlandHost = eval [
    {
      programs.hyprland.enable = true;
      services.tohru-greeter = {
        enable = true;
        style = "horizon-meniscus";
        preferredOutput = "DP-2";
        defaultUser = "jesse";
        defaultSession = "hyprland-uwsm";
        monitors = [
          { output = "DP-1"; mode = "1920x1080@144"; position = "0x0"; transform = 1; }
          { output = "DP-2"; mode = "1920x1080@360"; position = "1080x0"; }
        ];
        avatars.jesse = ../../LICENSE;
      };
    }
  ];

  swayHost = eval [
    {
      services.xserver.videoDrivers = [ "nvidia" ];
      services.tohru-greeter = {
        enable = true;
        compositor = "sway";
        allowPowerActions = false;
        fallbackToTui = false;
      };
    }
  ];

  hyprCmd = hyprlandHost.services.greetd.settings.default_session.command;
  swayCmd = swayHost.services.greetd.settings.default_session.command;
in
assert !disabled.services.greetd.enable;
assert !disabled.services.accounts-daemon.enable;
assert hyprlandHost.services.greetd.enable;
assert hyprlandHost.services.greetd.settings.default_session.user == "greeter";
assert hyprlandHost.services.tohru-greeter.compositor == "hyprland";
assert hyprlandHost.services.accounts-daemon.enable;
assert lib.elem "video" hyprlandHost.users.users.greeter.extraGroups;
assert lib.elem "render" hyprlandHost.users.users.greeter.extraGroups;
assert lib.any (r: lib.hasPrefix "d /var/lib/tohru-greeter " r) hyprlandHost.systemd.tmpfiles.rules;
assert lib.any (r: lib.hasPrefix "L+ /var/lib/AccountsService/icons/jesse " r) hyprlandHost.systemd.tmpfiles.rules;
assert lib.hasInfix "org.freedesktop.login1.power-off" hyprlandHost.security.polkit.extraConfig;
assert !(lib.hasInfix "power-off-multiple-sessions" hyprlandHost.security.polkit.extraConfig);
assert swayHost.services.greetd.enable;
assert swayHost.services.tohru-greeter.compositor == "sway";
assert swayHost.services.tohru-greeter.swayUnsupportedGpu;
assert !(lib.hasInfix "org.freedesktop.login1.power-off" swayHost.security.polkit.extraConfig);
{
  assertions = true;
  inherit hyprCmd swayCmd;
}

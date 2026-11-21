{
  description = "Tohru native Pleamar shell and reusable Home Manager module";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/4fe25e12eb9f4c744966dd3de5a05246cf098601";

  # Keep the optional plugin on E's reviewed Hyprland 0.57.2 package set.
  inputs.hyprland-nixpkgs.url = "github:NixOS/nixpkgs/2786a2a72088ecf32aea5b3ec52099e6a7cff78f";

  outputs = { self, nixpkgs, hyprland-nixpkgs }:
    let
      # Only the platform validated in this foundation checkpoint is exported.
      systems = [ "x86_64-linux" ];
      eachSystem = nixpkgs.lib.genAttrs systems;
    in {
      packages = eachSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
          tohru = pkgs.callPackage ./nix/default.nix { };
        in {
          default = tohru;
          inherit tohru;
          pleamar-runtime = tohru.runtime;
        } // nixpkgs.lib.optionalAttrs (builtins.pathExists ./hyprland/package.nix) {
          # This package must use the exact reviewed candidate compositor headers.
          tohru-hyprland = (import hyprland-nixpkgs { inherit system; }).callPackage ./hyprland/package.nix { };
        });
      apps = eachSystem (system: {
        default = self.apps.${system}.shell;
        shell = {
          type = "app";
          program = "${self.packages.${system}.tohru}/bin/tohru-run";
        };
        tohru = {
          type = "app";
          program = "${self.packages.${system}.tohru}/bin/tohru";
        };
      });
      # Hyprland plugins only load into the compositor they were built against.
      overlays.default = final: _prev: {
        tohru-hyprland = final.callPackage ./hyprland/package.nix { };
      };
      homeManagerModules = {
        tohru = import ./nix/home-manager.nix;
        default = self.homeManagerModules.tohru;
      };
      checks = eachSystem (system:
        let
          pkgs = import nixpkgs { inherit system; };
        in import ./nix/tests/compositors.nix {
          inherit pkgs;
          tohru = self.packages.${system}.tohru;
        });
      nixosModules = {
        tohru-greeter = import ./nix/nixos.nix;
        default = self.nixosModules.tohru-greeter;
      };
    };
}

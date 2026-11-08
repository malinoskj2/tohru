{
  description = "Tohru native Pleamar shell and reusable Home Manager module";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/4fe25e12eb9f4c744966dd3de5a05246cf098601";

  outputs = { self, nixpkgs }:
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
        } // nixpkgs.lib.optionalAttrs (builtins.pathExists ./effects/package.nix) {
          # This package must use the exact reviewed candidate compositor headers.
          desktop-effects = pkgs.callPackage ./effects/package.nix { };
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
      homeManagerModules = {
        tohru = import ./nix/home-manager.nix;
        default = self.homeManagerModules.tohru;
      };
    };
}

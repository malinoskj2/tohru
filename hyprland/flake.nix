{
  description = "Independent optional Hyprland desktop effects";
  inputs.nixpkgs.url = "github:nixos/nixpkgs/2786a2a72088ecf32aea5b3ec52099e6a7cff78f";
  outputs = { self, nixpkgs }:
    let
      systems = [ "x86_64-linux" "aarch64-linux" ];
      eachSystem = nixpkgs.lib.genAttrs systems;
    in {
      packages = eachSystem (system:
        let pkgs = import nixpkgs { inherit system; };
        in {
          default = pkgs.callPackage ./package.nix { };
          tohru-hyprland = self.packages.${system}.default;
        });
      devShells = eachSystem (system:
        let pkgs = import nixpkgs { inherit system; };
        in { default = pkgs.mkShell { inputsFrom = [ self.packages.${system}.default ]; }; });
    };
}

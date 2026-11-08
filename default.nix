# Supply an explicitly pinned nixpkgs package set from the integrating flake.
{ pkgs }:
pkgs.callPackage ./nix/default.nix { }

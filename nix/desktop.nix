{ lib, rustPlatform, bash }:
rustPlatform.buildRustPackage {
  pname = "tohru-desktop";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../desktop/Cargo.toml ../desktop/Cargo.lock ../desktop/src ../desktop/tests ../LICENSE ];
  };
  cargoRoot = "desktop";
  buildAndTestSubdir = "desktop";
  cargoLock.lockFile = ../desktop/Cargo.lock;
  nativeCheckInputs = [ bash ];
  meta = {
    description = "Tohru's desktop helpers, assistant and Discord broker";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}

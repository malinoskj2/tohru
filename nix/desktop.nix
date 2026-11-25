{ lib, rustPlatform, bash, dbus }:
rustPlatform.buildRustPackage {
  pname = "tohru-desktop";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../desktop/Cargo.toml ../desktop/Cargo.lock ../desktop/src ../desktop/tests ../share ../LICENSE ];
  };
  cargoRoot = "desktop";
  buildAndTestSubdir = "desktop";
  cargoLock.lockFile = ../desktop/Cargo.lock;
  nativeCheckInputs = [ bash dbus ];
  meta = {
    description = "Tohru's desktop helpers, assistant and Discord broker";
    license = lib.licenses.mit;
    platforms = lib.platforms.linux;
  };
}

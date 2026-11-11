{ lib, stdenvNoCC, rustPlatform, fetchurl, pkg-config, makeWrapper, callPackage
, fontconfig, wayland, libxkbcommon, linux-pam, libffi, vulkan-loader, libglvnd, python3 }:
let
  pin = builtins.fromJSON (builtins.readFile ../runtime/pleamar/pin.json);
  # Keep documentation/flake/test changes from rebuilding the full runtime.
  foundationSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../runtime/Cargo.toml ../runtime/Cargo.lock
      (lib.fileset.fileFilter (file: file.hasExt "rs") ../runtime/src) ../runtime/tests
      ../runtime/pleamar/bindings ../runtime/pleamar/tests/role-install.rs ../LICENSE ];
  };
  source = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../bin ../shell ../LICENSE ];
  };
  runtimeUnwrapped = rustPlatform.buildRustPackage {
    pname = "pleamar-tohru";
    version = pin.version;
    src = fetchurl { url = pin.archive; hash = pin.hash; };
    cargoLock.lockFile = ../runtime/pleamar/Cargo.lock;
    patches = map (name: ../runtime/pleamar/patches + "/${name}")
      (builtins.filter (lib.hasSuffix ".patch") (builtins.attrNames (builtins.readDir ../runtime/pleamar/patches)));
    postPatch = ''
      mkdir foundation
      cp ${foundationSource}/runtime/Cargo.toml ${foundationSource}/runtime/Cargo.lock foundation/
      cp -r ${foundationSource}/runtime/src ${foundationSource}/runtime/tests foundation/
      for binding in settings info; do
        cp ${foundationSource}/runtime/pleamar/bindings/''${binding}_luau.rs src/tohru_$binding.rs
      done
      mkdir -p tests/runtime
      cp ${foundationSource}/runtime/pleamar/tests/role-install.rs tests/runtime/
      cp ${../runtime/pleamar/Cargo.lock} Cargo.lock
    '';
    nativeBuildInputs = [ pkg-config makeWrapper ];
    nativeCheckInputs = [ python3 ];
    buildInputs = [ fontconfig wayland libxkbcommon linux-pam libffi ];
    # Owned native boundaries only; includes the isolated role-install regression.
    # Upstream's broader tests can require desktop services and remain separate.
    cargoTestFlags = [ "--lib" "tohru_" ];
    postInstall = ''
      mkdir -p $out/share/licenses/pleamar-tohru
      cp LICENSE THIRD-PARTY.md $out/share/licenses/pleamar-tohru/
      cp ${foundationSource}/LICENSE $out/share/licenses/pleamar-tohru/TOHRU-LICENSE
    '';
    postFixup = ''
      wrapProgram $out/bin/pleamar --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ vulkan-loader libglvnd wayland ]}
    '';
    meta = { license = [ lib.licenses.bsd3 lib.licenses.mit ]; platforms = lib.platforms.linux; };
  };
  runtime = callPackage ./runtime-wrapper.nix { runtime = runtimeUnwrapped; };
in stdenvNoCC.mkDerivation {
  pname = "tohru-shell";
  version = "0.1.0";
  src = source;
  nativeBuildInputs = [ makeWrapper ];
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/share/tohru
    cp -r shell $out/share/tohru/
    cp LICENSE $out/share/tohru/LICENSE
    cp bin/tohru bin/tohru-run bin/tohru-settings-run bin/tohru-session-run \
      bin/tohru-launcher-run bin/tohru-notifications-run $out/bin/
    wrapProgram $out/bin/tohru --set TOHRU_RUNTIME ${runtime}/bin/pleamar
    wrapProgram $out/bin/tohru-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SCENE $out/share/tohru/shell/bar.plm
    wrapProgram $out/bin/tohru-settings-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SETTINGS_SCENE $out/share/tohru/shell/settings/settings.plm
    wrapProgram $out/bin/tohru-session-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SESSION_SCENE $out/share/tohru/shell/session/menu.plm
    wrapProgram $out/bin/tohru-launcher-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_LAUNCHER_SCENE $out/share/tohru/shell/launcher/entry.plm
    wrapProgram $out/bin/tohru-notifications-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_NOTIFICATIONS_SCENE $out/share/tohru/shell/notifications/notifications.plm
    runHook postInstall
  '';
  passthru = { inherit runtime; };
  meta = { license = lib.licenses.mit; platforms = lib.platforms.linux; mainProgram = "tohru-run"; };
}

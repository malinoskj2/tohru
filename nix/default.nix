{ lib, stdenvNoCC, rustPlatform, fetchurl, pkg-config, makeWrapper, callPackage
, fontconfig, wayland, libxkbcommon, linux-pam, libffi, vulkan-loader, libglvnd, python3 }:
let
  pin = builtins.fromJSON (builtins.readFile ../runtime/pin.json);
  # Keep documentation/flake/test changes from rebuilding the full runtime.
  foundationSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ../runtime/settings_luau.rs ../runtime/workspace_luau.rs
      ../runtime/workspace/mod.rs ../runtime/workspace/workspace_motion.rs ../runtime/workspace/workspace_track.rs
      ../runtime/window/visibility.rs
      ../runtime/info_luau.rs ../runtime/info/mod.rs ../runtime/info/read.rs ../runtime/info/storage.rs ../runtime/info/system.rs ../tests/info/snapshot.rs ../tests/info.rs ../tests/settings.rs
        ../tests/runtime/role-install.rs ../LICENSE ];
  };
  lockSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../runtime/lock/native.rs ../runtime/lock/authority.rs ../runtime/lock/publication.rs
      ../tests/session/lock_authority.rs ../tests/session/lock_publication.rs ../tests/session/native_security.rs ../tests/session/native_binding.rs
      ../tests/session/security-owner.luau ../tests/session/security-owner.plm
      ../tests/session/security-public.plm ../tests/session/security-secret.plm
    ];
  };
  source = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../bin ../shell ../LICENSE ];
  };
  runtimeUnwrapped = rustPlatform.buildRustPackage {
    pname = "pleamar-tohru";
    version = pin.version;
    src = fetchurl { url = pin.archive; hash = pin.hash; };
    cargoLock.lockFile = ../runtime/Cargo.lock;
    patches = [ ../runtime/patches/0001-shell-settings.patch ../runtime/patches/0002-lock-boundary.patch
      ../runtime/workspace/shader-inputs.patch ../runtime/patches/0004-workspace-binding.patch
      ../runtime/launcher/namespace.patch ../runtime/patches/0003-window-lifecycle.patch ../runtime/patches/0006-info-binding.patch
        ../runtime/patches/0008-locker-gate.patch
        ../runtime/patches/0010-key-rule-order.patch ../runtime/patches/0011-focus-round.patch ];
    postPatch = ''
      mkdir foundation
      cp ${foundationSource}/Cargo.toml foundation/
      cp ${foundationSource}/Cargo.lock foundation/
      cp -r ${foundationSource}/src foundation/
      mkdir -p foundation/runtime
      cp -r ${foundationSource}/runtime/workspace foundation/runtime/
      cp -r ${foundationSource}/runtime/info foundation/runtime/
      mkdir -p foundation/tests/info
      cp ${foundationSource}/tests/info/snapshot.rs foundation/tests/info/
      cp ${foundationSource}/tests/info.rs ${foundationSource}/tests/settings.rs foundation/tests/
      cp ${foundationSource}/runtime/settings_luau.rs src/tohru_settings.rs
      cp ${foundationSource}/runtime/workspace_luau.rs src/tohru_workspace.rs
      cp ${foundationSource}/runtime/window/visibility.rs src/tohru_window.rs
      cp ${foundationSource}/runtime/info_luau.rs src/tohru_info.rs
      mkdir -p src/tohru_lock tests/session
      cp ${lockSource}/runtime/lock/native.rs src/tohru_lock.rs
      cp ${lockSource}/runtime/lock/authority.rs src/tohru_lock/authority.rs
      cp ${lockSource}/runtime/lock/publication.rs src/tohru_lock/publication.rs
      cp ${lockSource}/tests/session/* tests/session/
      mkdir -p tests/runtime
      cp ${foundationSource}/tests/runtime/role-install.rs tests/runtime/
      cp ${../runtime/Cargo.lock} Cargo.lock
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
    cp bin/tohru bin/tohru-run bin/tohru-locker-run $out/bin/
    wrapProgram $out/bin/tohru --set TOHRU_RUNTIME ${runtime}/bin/pleamar
    wrapProgram $out/bin/tohru-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SCENE $out/share/tohru/shell/bar.plm
    wrapProgram $out/bin/tohru-locker-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_LOCK_SCENE $out/share/tohru/shell/lock/lock.plm
    runHook postInstall
  '';
  passthru = { inherit runtime; };
  meta = { license = lib.licenses.mit; platforms = lib.platforms.linux; mainProgram = "tohru-run"; };
}

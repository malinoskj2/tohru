{ lib, stdenvNoCC, rustPlatform, fetchurl, pkg-config, makeWrapper, callPackage
, fontconfig, wayland, libxkbcommon, linux-pam, libffi, vulkan-loader, libglvnd }:
let
  pin = builtins.fromJSON (builtins.readFile ../runtime/pin.json);
  # Keep documentation/flake/test changes from rebuilding the full runtime.
  foundationSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ../runtime/settings_luau.rs ../runtime/workspace_luau.rs
      ../runtime/workspace/mod.rs ../runtime/workspace/workspace_motion.rs ../runtime/workspace/workspace_track.rs
      ../runtime/wallpaper/mod.rs ../runtime/wallpaper/probe.rs ../runtime/wallpaper/native.rs
      ../runtime/window/visibility.rs
      ../runtime/info_luau.rs ../runtime/info/mod.rs ../runtime/info/read.rs ../runtime/info/storage.rs ../runtime/info/system.rs ../tests/info/snapshot.rs ../tests/info.rs
      ../runtime/capture/mod.rs ../runtime/capture/files.rs ../runtime/capture/geometry.rs
      ../runtime/capture/native.rs ../runtime/capture/clipboard.rs ../runtime/capture/clipboard_owner.rs
      ../tests/wallpaper/catalog.rs ../tests/capture
      ../runtime/idle/mod.rs ../runtime/idle/cookies.rs ../runtime/idle/delay.rs ../runtime/idle/input.rs ../runtime/idle/logind_state.rs
      ../runtime/idle/native.rs ../runtime/idle/observer.rs ../runtime/idle/screensaver.rs ../runtime/idle/logind.rs ../runtime/idle/sleep_guard.rs
      ../tests/session/idle.rs ../tests/session/idle_cookies.rs ../tests/session/idle_delay.rs ../tests/session/idle_input.rs ../tests/session/idle_logind.rs ../LICENSE ];
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
      ../runtime/wallpaper/catalog.patch ../runtime/wallpaper/image-reset.patch ../runtime/patches/0005-wallpaper-module.patch
      ../runtime/launcher/namespace.patch ../runtime/patches/0003-window-lifecycle.patch ../runtime/patches/0006-info-binding.patch
      ../runtime/capture/native.patch ../runtime/capture/layout-link.patch
      ../runtime/capture/image-lifecycle.patch ../runtime/capture/output-events.patch
      ../runtime/patches/0007-capture-assembly.patch
      ../runtime/idle/0004-idle-native.patch ../runtime/patches/0008-idle-binding.patch ];
    postPatch = ''
      mkdir foundation
      cp ${foundationSource}/Cargo.toml foundation/
      cp ${foundationSource}/Cargo.lock foundation/
      cp -r ${foundationSource}/src foundation/
      mkdir -p foundation/runtime foundation/tests/session
      mkdir -p foundation/runtime/idle
      cp ${foundationSource}/runtime/idle/mod.rs ${foundationSource}/runtime/idle/cookies.rs ${foundationSource}/runtime/idle/delay.rs ${foundationSource}/runtime/idle/input.rs ${foundationSource}/runtime/idle/logind_state.rs foundation/runtime/idle/
      cp -r ${foundationSource}/runtime/workspace foundation/runtime/
      mkdir -p foundation/runtime/capture
      cp ${foundationSource}/runtime/capture/mod.rs ${foundationSource}/runtime/capture/files.rs ${foundationSource}/runtime/capture/geometry.rs foundation/runtime/capture/
      cp -r ${foundationSource}/runtime/info foundation/runtime/
      mkdir -p foundation/tests/info
      cp ${foundationSource}/tests/info/snapshot.rs foundation/tests/info/
      cp ${foundationSource}/tests/info.rs foundation/tests/
      mkdir -p foundation/runtime/wallpaper foundation/tests/wallpaper foundation/tests/capture
      cp ${foundationSource}/runtime/wallpaper/mod.rs ${foundationSource}/runtime/wallpaper/probe.rs foundation/runtime/wallpaper/
      cp ${foundationSource}/tests/wallpaper/catalog.rs foundation/tests/wallpaper/
      cp ${foundationSource}/tests/capture/*.rs foundation/tests/capture/
      cp ${foundationSource}/tests/session/idle.rs foundation/tests/session/
      cp ${foundationSource}/tests/session/idle_cookies.rs ${foundationSource}/tests/session/idle_delay.rs ${foundationSource}/tests/session/idle_input.rs ${foundationSource}/tests/session/idle_logind.rs foundation/tests/session/
      cp ${foundationSource}/runtime/settings_luau.rs src/tohru_settings.rs
      cp ${foundationSource}/runtime/workspace_luau.rs src/tohru_workspace.rs
      cp ${foundationSource}/runtime/wallpaper/native.rs src/tohru_wallpaper.rs
      cp ${foundationSource}/runtime/window/visibility.rs src/tohru_window.rs
      cp ${foundationSource}/runtime/info_luau.rs src/tohru_info.rs
      mkdir -p src/tohru_idle
      cp ${foundationSource}/runtime/idle/native.rs src/tohru_idle.rs
      cp ${foundationSource}/runtime/idle/observer.rs ${foundationSource}/runtime/idle/screensaver.rs ${foundationSource}/runtime/idle/logind.rs ${foundationSource}/runtime/idle/sleep_guard.rs src/tohru_idle/
      mkdir -p src/platform/tohru_capture
      cp ${foundationSource}/runtime/capture/native.rs ${foundationSource}/runtime/capture/clipboard_owner.rs src/platform/tohru_capture/
      cp ${foundationSource}/runtime/capture/clipboard.rs src/tohru_capture_clipboard.rs
      mkdir -p src/tohru_lock tests/session
      cp ${lockSource}/runtime/lock/native.rs src/tohru_lock.rs
      cp ${lockSource}/runtime/lock/authority.rs src/tohru_lock/authority.rs
      cp ${lockSource}/runtime/lock/publication.rs src/tohru_lock/publication.rs
      cp ${lockSource}/tests/session/* tests/session/
      cp ${../runtime/Cargo.lock} Cargo.lock
    '';
    nativeBuildInputs = [ pkg-config makeWrapper ];
    buildInputs = [ fontconfig wayland libxkbcommon linux-pam libffi ];
    # Upstream's broader tests include desktop services; run our narrow binding test.
    cargoTestFlags = [ "--lib" "tohru_settings::" ];
    postBuild = ''
      cargo build --offline --locked --release --jobs 1 \
        --bin tohru-capture-clipboard --target ${stdenvNoCC.hostPlatform.rust.rustcTarget}
      cargo build --offline --locked --release --jobs 1 \
        --manifest-path foundation/Cargo.toml --bin tohru-wallpaper-probe \
        --target ${stdenvNoCC.hostPlatform.rust.rustcTarget} --target-dir target
    '';
    postInstall = ''
      mkdir -p $out/share/licenses/pleamar-tohru
      cp LICENSE THIRD-PARTY.md $out/share/licenses/pleamar-tohru/
      cp ${foundationSource}/LICENSE $out/share/licenses/pleamar-tohru/TOHRU-LICENSE
      install -m755 target/${stdenvNoCC.hostPlatform.rust.rustcTarget}/release/tohru-wallpaper-probe $out/bin/tohru-wallpaper-probe
      install -m755 target/${stdenvNoCC.hostPlatform.rust.rustcTarget}/release/tohru-capture-clipboard $out/bin/tohru-capture-clipboard
    '';
    postFixup = ''
      wrapProgram $out/bin/pleamar --prefix LD_LIBRARY_PATH : ${lib.makeLibraryPath [ vulkan-loader libglvnd ]}
    '';
    meta = { license = lib.licenses.bsd3; platforms = lib.platforms.linux; };
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
    cp bin/tohru bin/tohru-run $out/bin/
    wrapProgram $out/bin/tohru --set TOHRU_RUNTIME ${runtime}/bin/pleamar
    wrapProgram $out/bin/tohru-run \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SCENE $out/share/tohru/shell/bar.plm
    runHook postInstall
  '';
  passthru = { inherit runtime; };
  meta = { license = lib.licenses.mit; platforms = lib.platforms.linux; mainProgram = "tohru-run"; };
}

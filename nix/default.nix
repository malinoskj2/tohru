{ lib, stdenvNoCC, rustPlatform, fetchurl, pkg-config, makeWrapper, callPackage
, fontconfig, wayland, libxkbcommon, linux-pam, libffi, vulkan-loader, libglvnd, mpv-unwrapped, python3 }:
let
  pin = builtins.fromJSON (builtins.readFile ../runtime/pin.json);
  # Keep documentation/flake/test changes from rebuilding the full runtime.
  foundationSource = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [ ../Cargo.toml ../Cargo.lock ../src ../runtime/settings_luau.rs ../runtime/workspace_luau.rs
      ../runtime/workspace/mod.rs ../runtime/workspace/workspace_motion.rs ../runtime/workspace/workspace_track.rs
      ../runtime/wallpaper/mod.rs ../runtime/wallpaper/probe.rs ../runtime/wallpaper/native.rs
      ../runtime/wallpaper/preset.rs ../runtime/wallpaper/presets.rs ../runtime/wallpaper/raster.rs ../runtime/wallpaper/J2BAR-LICENSE
      ../runtime/window/visibility.rs
      ../runtime/info_luau.rs ../runtime/info/mod.rs ../runtime/info/read.rs ../runtime/info/storage.rs ../runtime/info/system.rs ../tests/info/snapshot.rs ../tests/info.rs ../tests/settings.rs
      ../runtime/capture/mod.rs ../runtime/capture/files.rs ../runtime/capture/geometry.rs
      ../runtime/capture/native.rs ../runtime/capture/clipboard.rs ../runtime/capture/clipboard_owner.rs
      ../runtime/video ../runtime/video_luau.rs ../tests/wallpaper/video.rs ../tests/wallpaper/mpv.rs ../tests/wallpaper/video_startup.rs
      ../tests/wallpaper/catalog.rs ../tests/wallpaper/preset.rs ../tests/wallpaper/presets.rs ../tests/wallpaper/raster.rs ../tests/capture
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
      ../runtime/wallpaper/catalog.patch ../runtime/wallpaper/image-reset.patch ../runtime/wallpaper/fit.patch ../runtime/patches/0005-wallpaper-module.patch
      ../runtime/launcher/namespace.patch ../runtime/patches/0003-window-lifecycle.patch ../runtime/patches/0006-info-binding.patch
      ../runtime/capture/native.patch ../runtime/capture/layout-link.patch
      ../runtime/capture/image-lifecycle.patch ../runtime/wallpaper/raster-limits.patch
      ../runtime/wallpaper/presets.patch ../runtime/patches/0009-preset-dependency.patch ../runtime/capture/output-events.patch
      ../runtime/patches/0007-capture-assembly.patch
      ../runtime/patches/0008-locker-gate.patch
      ../runtime/patches/0010-key-rule-order.patch ../runtime/patches/0011-focus-round.patch
      ../runtime/patches/0012-video-binding.patch ];
    postPatch = ''
      mkdir foundation
      cp ${foundationSource}/Cargo.toml foundation/
      cp ${foundationSource}/Cargo.lock foundation/
      cp -r ${foundationSource}/src foundation/
      mkdir -p foundation/runtime
      cp -r ${foundationSource}/runtime/workspace foundation/runtime/
      mkdir -p foundation/runtime/capture
      cp ${foundationSource}/runtime/capture/mod.rs ${foundationSource}/runtime/capture/files.rs ${foundationSource}/runtime/capture/geometry.rs foundation/runtime/capture/
      cp -r ${foundationSource}/runtime/info foundation/runtime/
      mkdir -p foundation/tests/info
      cp ${foundationSource}/tests/info/snapshot.rs foundation/tests/info/
      cp ${foundationSource}/tests/info.rs ${foundationSource}/tests/settings.rs foundation/tests/
      mkdir -p foundation/runtime/wallpaper foundation/tests/wallpaper foundation/tests/capture
      cp ${foundationSource}/runtime/wallpaper/mod.rs ${foundationSource}/runtime/wallpaper/probe.rs ${foundationSource}/runtime/wallpaper/preset.rs ${foundationSource}/runtime/wallpaper/J2BAR-LICENSE foundation/runtime/wallpaper/
      cp ${foundationSource}/tests/wallpaper/catalog.rs ${foundationSource}/tests/wallpaper/preset.rs foundation/tests/wallpaper/
      cp ${foundationSource}/tests/capture/*.rs foundation/tests/capture/
      mkdir -p foundation/runtime/video
      cp ${foundationSource}/runtime/video/mod.rs foundation/runtime/video/
      cp ${foundationSource}/tests/wallpaper/video.rs foundation/tests/wallpaper/
      cp ${foundationSource}/runtime/settings_luau.rs src/tohru_settings.rs
      cp ${foundationSource}/runtime/workspace_luau.rs src/tohru_workspace.rs
      mkdir -p src/tohru_wallpaper tests/wallpaper
      cp ${foundationSource}/runtime/wallpaper/native.rs src/tohru_wallpaper/mod.rs
      # The native catalog transport tests generate Python fixtures at runtime;
      # patch their literal shebang because Nix has no /usr/bin/env interpreter.
      substituteInPlace src/tohru_wallpaper/mod.rs \
        --replace-fail '#!/usr/bin/env python3' '#!${python3}/bin/python3'
      cp ${foundationSource}/runtime/wallpaper/presets.rs ${foundationSource}/runtime/wallpaper/raster.rs src/tohru_wallpaper/
      cp ${foundationSource}/tests/wallpaper/presets.rs ${foundationSource}/tests/wallpaper/raster.rs tests/wallpaper/
      cp ${foundationSource}/runtime/window/visibility.rs src/tohru_window.rs
      cp ${foundationSource}/runtime/info_luau.rs src/tohru_info.rs
      mkdir -p src/platform/tohru_capture
      cp ${foundationSource}/runtime/capture/native.rs ${foundationSource}/runtime/capture/clipboard_owner.rs src/platform/tohru_capture/
      cp ${foundationSource}/runtime/capture/clipboard.rs src/tohru_capture_clipboard.rs
      mkdir -p src/tohru_video
      cp ${foundationSource}/runtime/video/native.rs src/tohru_video/mod.rs
      cp ${foundationSource}/runtime/video/{egl,mpv,startup,wake,wayland}.rs src/tohru_video/
      cp ${foundationSource}/tests/wallpaper/mpv.rs ${foundationSource}/tests/wallpaper/video_startup.rs tests/wallpaper/
      cp ${foundationSource}/runtime/video_luau.rs src/tohru_video_luau.rs
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
    buildInputs = [ fontconfig wayland libxkbcommon linux-pam libffi mpv-unwrapped ];
    # Owned native boundaries only; includes the isolated role-install regression.
    # Upstream's broader tests can require desktop services and remain separate.
    cargoTestFlags = [ "--lib" "tohru_" ];
    # Future allocated package checks keep native preset fixture images in this
    # private build tree; no cache/PNG test runs during source preparation.
    preCheck = ''
      export TOHRU_TEST_ARTIFACT_DIR="$PWD/.tmp"
      mkdir -p "$TOHRU_TEST_ARTIFACT_DIR"
      chmod 700 "$TOHRU_TEST_ARTIFACT_DIR"
    '';
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
      cp ${foundationSource}/runtime/wallpaper/J2BAR-LICENSE $out/share/licenses/pleamar-tohru/J2BAR-WALLPAPER-LICENSE
      cp ${foundationSource}/runtime/video/MPV-CLIENT-LICENSE $out/share/licenses/pleamar-tohru/MPV-CLIENT-LICENSE
      install -m755 target/${stdenvNoCC.hostPlatform.rust.rustcTarget}/release/tohru-wallpaper-probe $out/bin/tohru-wallpaper-probe
      install -m755 target/${stdenvNoCC.hostPlatform.rust.rustcTarget}/release/tohru-capture-clipboard $out/bin/tohru-capture-clipboard
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

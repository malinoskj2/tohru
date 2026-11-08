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
      ../runtime/capture ../tests/wallpaper/catalog.rs ../tests/capture
      ../runtime/idle ../tests/session/idle.rs ../LICENSE ];
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
      ../runtime/wallpaper/catalog.patch ../runtime/wallpaper/image-reset.patch ../runtime/patches/0005-wallpaper-module.patch ];
    postPatch = ''
      mkdir foundation
      cp ${foundationSource}/Cargo.toml foundation/
      cp ${foundationSource}/Cargo.lock foundation/
      cp -r ${foundationSource}/src foundation/
      mkdir -p foundation/runtime foundation/tests/session
      cp -r ${foundationSource}/runtime/idle foundation/runtime/
      cp -r ${foundationSource}/runtime/workspace foundation/runtime/
      cp -r ${foundationSource}/runtime/capture foundation/runtime/
      mkdir -p foundation/runtime/wallpaper foundation/tests/wallpaper foundation/tests/capture
      cp ${foundationSource}/runtime/wallpaper/mod.rs ${foundationSource}/runtime/wallpaper/probe.rs foundation/runtime/wallpaper/
      cp ${foundationSource}/tests/wallpaper/catalog.rs foundation/tests/wallpaper/
      cp ${foundationSource}/tests/capture/*.rs foundation/tests/capture/
      cp ${foundationSource}/tests/session/idle.rs foundation/tests/session/
      cp ${foundationSource}/runtime/settings_luau.rs src/tohru_settings.rs
      cp ${foundationSource}/runtime/workspace_luau.rs src/tohru_workspace.rs
      cp ${foundationSource}/runtime/wallpaper/native.rs src/tohru_wallpaper.rs
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
        --manifest-path foundation/Cargo.toml --bin tohru-wallpaper-probe \
        --target ${stdenvNoCC.hostPlatform.rust.rustcTarget} --target-dir target
    '';
    postInstall = ''
      mkdir -p $out/share/licenses/pleamar-tohru
      cp LICENSE THIRD-PARTY.md $out/share/licenses/pleamar-tohru/
      cp ${foundationSource}/LICENSE $out/share/licenses/pleamar-tohru/TOHRU-LICENSE
      install -m755 target/${stdenvNoCC.hostPlatform.rust.rustcTarget}/release/tohru-wallpaper-probe $out/bin/tohru-wallpaper-probe
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

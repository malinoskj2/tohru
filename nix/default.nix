{ lib, stdenvNoCC, rustPlatform, fetchurl, pkg-config, makeWrapper, callPackage
, fontconfig, wayland, jq, socat, ffmpeg-headless, findutils, gnugrep, curl, libxkbcommon, linux-pam, libffi, vulkan-loader, libglvnd, python3, dbus
, libnotify, util-linux, systemd, procps, coreutils, espeak-ng, pulseaudio, wayland-utils, swayidle, mpvpaper, wl-clipboard, xdg-terminal-exec
  # Without the voice service nothing for speech is installed; the setting then has nothing to start.
, withVoice ? true }:
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
    fileset = lib.fileset.unions [ ../bin ../shell ../share ../docs ../LICENSE ];
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
      cp ${foundationSource}/runtime/pleamar/bindings/lock.rs src/tohru_lock.rs
      cp ${foundationSource}/runtime/pleamar/bindings/greeter.rs src/tohru_greeter.rs
      mkdir -p tests/runtime
      cp ${foundationSource}/runtime/pleamar/tests/role-install.rs tests/runtime/
      cp ${../runtime/pleamar/Cargo.lock} Cargo.lock
    '';
    nativeBuildInputs = [ pkg-config makeWrapper ];
    nativeCheckInputs = [ python3 dbus ];
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
  # misaki's English G2P with its espeak fallback; its optional neural fallback (torch) is never loaded.
  portalPython = python3.withPackages (p: [ p.dbus-fast ]);
  voicePython = python3.withPackages (p: [
    p.numpy p.onnxruntime p.sherpa-onnx p.misaki p.num2words p.phonemizer p.spacy-models.en_core_web_sm
  ]);
in stdenvNoCC.mkDerivation {
  pname = "tohru-shell";
  version = "0.1.0";
  src = source;
  nativeBuildInputs = [ makeWrapper ];
  buildInputs = [ python3 ];
  dontBuild = true;
  installPhase = ''
    runHook preInstall
    mkdir -p $out/bin $out/share/tohru
    cp -r shell $out/share/tohru/
    cp -r share/tohru/. $out/share/tohru/
    cp -r docs $out/share/tohru/
    cp bin/tohru_*.py bin/tohru-idle bin/tohru-session-action bin/tohru-wallpaper-media $out/bin/
    cp LICENSE $out/share/tohru/LICENSE
    cp bin/tohru bin/tohru-run bin/tohru-settings-run bin/tohru-session-run \
      bin/tohru-launcher-run bin/tohru-notifications-run bin/tohru-lock bin/tohru-greeter bin/tohru-wallpaper bin/tohru-media-art bin/tohru-polkit-run bin/tohru-dev bin/tohru-assistant bin/tohru-discord bin/tohru_discord.py $out/bin/
    wrapProgram $out/bin/tohru --set TOHRU_RUNTIME ${runtime}/bin/pleamar --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}"
    wrapProgram $out/bin/tohru-assistant --prefix PATH : ${lib.makeBinPath [ libnotify ]}
    wrapProgram $out/bin/tohru-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SCENE $out/share/tohru/shell/bar.plm
    wrapProgram $out/bin/tohru-settings-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SETTINGS_SCENE $out/share/tohru/shell/settings/settings.plm
    wrapProgram $out/bin/tohru-session-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_SESSION_SCENE $out/share/tohru/shell/session/menu.plm
    wrapProgram $out/bin/tohru-launcher-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_LAUNCHER_SCENE $out/share/tohru/shell/launcher/entry.plm
    wrapProgram $out/bin/tohru-notifications-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_NOTIFICATIONS_SCENE $out/share/tohru/shell/notifications/notifications.plm
    wrapProgram $out/bin/tohru-lock \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_LOCK_SCENE $out/share/tohru/shell/lock/lock.plm \
      --set TOHRU_LOCK_PLAIN_SCENE $out/share/tohru/shell/lock/plain.plm \
      --prefix PATH : ${lib.makeBinPath [ util-linux systemd procps coreutils ]}
    wrapProgram $out/bin/tohru-greeter \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_GREETER_SCENE $out/share/tohru/shell/greeter/greeter.plm \
      --set TOHRU_GREETER_PLAIN_SCENE $out/share/tohru/shell/greeter/plain.plm \
      --prefix PATH : ${lib.makeBinPath [ systemd coreutils ]}
    # Generic output discovery uses wayland-info; Hyprland IPC remains optional.
    wrapProgram $out/bin/tohru-wallpaper \
      --prefix PATH : ${lib.makeBinPath [ python3 wayland-utils mpvpaper jq socat ffmpeg-headless util-linux coreutils findutils gnugrep ]}
    wrapProgram $out/bin/tohru-media-art \
      --prefix PATH : ${lib.makeBinPath [ curl coreutils ]}
    install -Dm755 bin/tohru-shortcuts $out/share/tohru/libexec/tohru-shortcuts
    cp bin/tohru_shortcuts.py bin/tohru_portability.py $out/share/tohru/libexec/
    makeWrapper ${portalPython}/bin/python3 $out/bin/tohru-shortcuts \
      --add-flags $out/share/tohru/libexec/tohru-shortcuts \
      --set TOHRU_BIN_DIR $out/bin \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ systemd coreutils ]}"
    ${lib.optionalString withVoice ''
      install -Dm755 bin/tohru-voice $out/share/tohru/libexec/tohru-voice
      cp bin/tohru_portability.py $out/share/tohru/libexec/
      makeWrapper ${voicePython}/bin/python3 $out/bin/tohru-voice \
        --add-flags $out/share/tohru/libexec/tohru-voice \
        --prefix PATH : ${lib.makeBinPath [ pulseaudio espeak-ng ]} \
        --set TOHRU_BIN_DIR $out/bin \
        --set OPENBLAS_NUM_THREADS 1 \
        --set HF_HUB_OFFLINE 1
    ''}
    wrapProgram $out/bin/tohru-polkit-run \
      --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}" \
      --set TOHRU_RUNTIME ${runtime}/bin/pleamar \
      --set TOHRU_POLKIT_SCENE $out/share/tohru/shell/polkit/polkit.plm
    for program in tohru-idle tohru-session-action; do
      wrapProgram $out/bin/$program --prefix PATH : "$out/bin:${lib.makeBinPath [ python3 fontconfig util-linux systemd coreutils procps wayland-utils swayidle mpvpaper wl-clipboard xdg-terminal-exec ]}"
    done
    runHook postInstall
  '';
  passthru = { inherit runtime voicePython withVoice; };
  meta = { license = lib.licenses.mit; platforms = lib.platforms.linux; mainProgram = "tohru-run"; };
}

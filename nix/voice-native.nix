{
  lib,
  stdenv,
  rustPlatform,
  fetchurl,
  makeWrapper,
  onnxruntime,
  pulseaudio,
  espeak-ng,
}:
let
  # Native implementation code only. Model files always remain user configured.
  sherpaLibraries = stdenv.mkDerivation {
    pname = "sherpa-onnx-native-static";
    version = "2.0.3";
    src = fetchurl {
      url = "https://github.com/k2-fsa/sherpa-onnx/releases/download/v2.0.3/sherpa-onnx-v2.0.3-linux-x64-static-lib.tar.bz2";
      hash = "sha256-+SfmldKbjH6j/al+7SFSBLIQ/Vf5+vmQqTN3usCYrVc=";
    };
    dontBuild = true;
    installPhase = ''
      mkdir -p $out/lib $out/share/licenses/sherpa-onnx
      cp lib/*.a $out/lib/
      if test -f LICENSE; then cp LICENSE $out/share/licenses/sherpa-onnx/; fi
    '';
    meta = {
      license = lib.licenses.asl20;
      platforms = [ "x86_64-linux" ];
    };
  };
in
rustPlatform.buildRustPackage {
  pname = "tohru-voice-native";
  version = "0.1.0";
  src = lib.fileset.toSource {
    root = ../.;
    fileset = lib.fileset.unions [
      ../voice/Cargo.toml
      ../voice/Cargo.lock
      ../voice/src
      ../voice/THIRD-PARTY.md
      ../voice/licenses
      ../shell/assistant/voice
      ../bin/tohru-voice-qwen3-worker
      ../LICENSE
    ];
  };
  cargoRoot = "voice";
  buildAndTestSubdir = "voice";
  cargoLock.lockFile = ../voice/Cargo.lock;
  SHERPA_ONNX_LIB_DIR = "${sherpaLibraries}/lib";
  nativeBuildInputs = [ makeWrapper ];
  postInstall = ''
    mkdir -p $out/share/tohru/voice
    cp shell/assistant/voice/{tohru-tide.bin,wake-keywords.txt,README.md} $out/share/tohru/voice/
    install -Dm644 bin/tohru-voice-qwen3-worker $out/libexec/tohru-voice-qwen3-worker
    mkdir -p $out/share/licenses/tohru-voice-native
    cp LICENSE voice/THIRD-PARTY.md voice/licenses/* $out/share/licenses/tohru-voice-native/
    wrapProgram $out/bin/tohru-voice-native \
      --set ORT_DYLIB_PATH ${onnxruntime}/lib/libonnxruntime.so \
      --prefix PATH : ${
        lib.makeBinPath [
          pulseaudio
          espeak-ng
        ]
      } \
      --add-flags "--assets $out/share/tohru/voice --high-worker $out/libexec/tohru-voice-qwen3-worker"
  '';
  meta = {
    description = "Tohru voice service: wake, turn taking, recognition and speech";
    license = [
      lib.licenses.mit
      lib.licenses.asl20
    ];
    platforms = [ "x86_64-linux" ];
    mainProgram = "tohru-voice-native";
  };
}

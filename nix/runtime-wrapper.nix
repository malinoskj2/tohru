{ lib, stdenvNoCC, makeWrapper, util-linux, bash, runtime }:

# Native apps.launch invokes setsid -f sh -c. Keep these dependencies outside
# the expensive Rust derivation, and retain the user's application search path.
stdenvNoCC.mkDerivation {
  pname = "pleamar-tohru-launch-wrapper";
  inherit (runtime) version;
  dontUnpack = true;
  nativeBuildInputs = [ makeWrapper ];
  installPhase = ''
    mkdir -p $out/bin
    makeWrapper ${runtime}/bin/pleamar $out/bin/pleamar \
      --prefix PATH : ${lib.makeBinPath [ util-linux bash ]}
    ln -s ${runtime}/share $out/share
  '';
  passthru.unwrapped = runtime;
  meta = { license = lib.licenses.bsd3; platforms = lib.platforms.linux; mainProgram = "pleamar"; };
}

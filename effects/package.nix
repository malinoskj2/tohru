{ hyprland, hyprlandPlugins, lib }:
assert lib.assertMsg (hyprland.version == "0.56.2")
  "tohru-hyprland requires reviewed Hyprland 0.56.2 headers; review private hooks before updating";
hyprlandPlugins.mkHyprlandPlugin {
  pluginName = "tohru-hyprland";
  version = "0.1.0";
  # Explicitly use the caller's compositor, including its existing source patches.
  inherit hyprland;
  src = lib.cleanSource ./.;
  dontStrip = true;
  buildPhase = ''
    runHook preBuild
    make -j1
    runHook postBuild
  '';
  doCheck = true;
  checkPhase = ''
    runHook preCheck
    make check-abi check-focus check-sheet
    runHook postCheck
  '';
  installPhase = ''
    runHook preInstall
    make install PREFIX=$out
    install -Dm644 README.md $out/share/doc/tohru-hyprland/README.md
    install -Dm644 UPSTREAM.md $out/share/doc/tohru-hyprland/UPSTREAM.md
    install -Dm644 source-manifest.json $out/share/doc/tohru-hyprland/source-manifest.json
    runHook postInstall
  '';
  meta = {
    description = "Optional focus, application titlebar, file chooser and launcher effects for Hyprland";
    license = [ lib.licenses.mit lib.licenses.bsd3 ];
    platforms = lib.platforms.linux;
  };
}

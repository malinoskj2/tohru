{ pkgs, tohru, hyprlandPackage }:

let
  password = "tohru-test";
  argvFile = "/tmp/tohru-test-session-argv";

  sessionScript = pkgs.writeShellScript "tohru-test-session" ''
    printf '%s\n' "$@" > ${argvFile}
    while :; do sleep 1000; done
  '';

  # The tokens need quoting once greetd space-joins the command for `sh -c`.
  testSession = (pkgs.writeTextDir "share/wayland-sessions/tohru-test.desktop" ''
    [Desktop Entry]
    Name=Tohru Test
    Exec=${sessionScript} "two words" "$dollar" plain
    DesktopNames=TohruTest
  '').overrideAttrs { passthru.providedSessions = [ "tohru-test" ]; };

  greeterTest = compositor: pkgs.testers.runNixOSTest {
    name = "tohru-greeter-${compositor}";

    nodes.machine = {
      imports = [ ../nixos.nix ];

      virtualisation = {
        memorySize = 4096;
        cores = 4;
        qemu.options = [ "-vga none" "-device virtio-gpu-pci" ];
      };

      hardware.graphics.enable = true;
      environment.variables = {
        WLR_RENDERER = "pixman";
        # Hyprland's llvmpipe renderer can't import Pleamar's own dmabuf frames and kills the greeter.
        PLEAMAR_SWAPCHAIN = "1";
      };
      fonts.packages = [ pkgs.dejavu_fonts ];
      documentation.enable = false;

      users.users.alice = {
        isNormalUser = true;
        inherit password;
      };

      services.displayManager.sessionPackages = [ testSession ];

      services.tohru-greeter = {
        enable = true;
        package = tohru;
        inherit compositor hyprlandPackage;
        defaultUser = "alice";
        defaultSession = "tohru-test";
        monitors = [{ output = "Virtual-1"; mode = "1280x800"; position = "0x0"; }];
        fallbackToTui = false;
      };
    };

    testScript = ''
      def dump_logs():
          for cmd in [
              "journalctl -b --no-pager -o short-monotonic -u greetd -n 60",
              "journalctl -b --no-pager -o short-monotonic _UID=$(id -u greeter) -n 150",
              "grep -hv aquamarine /run/user/*/hypr/*/hyprland.log | tail -n 80",
          ]:
              print(machine.execute(cmd)[1])

      try:
          machine.wait_for_unit("greetd.service")
          machine.wait_until_succeeds("pgrep -u greeter -f bin/pleamar", timeout=180)
          machine.sleep(15)
          machine.screenshot("greeter")
          machine.fail("journalctl -b --no-pager -o cat -u greetd | grep -F 'greeter exited without creating a session'")
          machine.succeed("pgrep -u greeter -f bin/pleamar")

          machine.send_chars("${password}\n")
          machine.wait_until_succeeds(
              "journalctl -b --no-pager -o cat | grep -F 'session opened for user alice'",
              timeout=60,
          )
          machine.wait_until_succeeds("test -s ${argvFile}", timeout=60)
          argv = machine.succeed("cat ${argvFile}")
          assert argv == "two words\n$dollar\nplain\n", f"session argv: {argv!r}"

          machine.wait_until_fails("pgrep -u greeter", timeout=60)
          machine.sleep(15)
          machine.succeed("pgrep -u alice -f tohru-test-session")
          opened = machine.succeed(
              "journalctl -b --no-pager -o cat | grep -cF 'pam_unix(greetd:session): session opened for user alice'"
          ).strip()
          assert opened == "1", f"alice sessions opened: {opened}"
          machine.fail("pgrep -u greeter")
          machine.fail("journalctl -b --no-pager -o cat | grep -F 'pam_unix(greetd:session): session closed for user alice'")
      except Exception:
          dump_logs()
          raise
    '';
  };
in {
  greeter-hyprland = greeterTest "hyprland";
  greeter-sway = greeterTest "sway";
}

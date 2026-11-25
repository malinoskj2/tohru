/// Read focused Hyprland geometry during one entrance. No titles or captures.
fn main() -> anyhow::Result<()> {
    let actor = std::env::args().skip(1).eq(["--actor".to_owned()]);
    tohru_desktop::flight_scene::run(actor)
}

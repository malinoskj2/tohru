use super::*;
#[test]
fn fixed_names_cycle_palettes_and_cache_key_count_are_closed() {
    let mut keys = std::collections::BTreeSet::new();
    for preset in Preset::ALL {
        assert_eq!(preset.name().parse::<Preset>().unwrap(), preset);
        for palette in Palette::ALL {
            assert_eq!(palette.name().parse::<Palette>().unwrap(), palette);
            keys.insert(format!("{}-{}.png", preset.name(), palette.name()));
        }
    }
    assert_eq!(keys.len(), 48);
    for invalid in ["../solid", "Solid", "", "unknown"] {
        assert!(invalid.parse::<Preset>().is_err());
    }
    for invalid in ["../mocha", "custom", "#000000", ""] {
        assert!(invalid.parse::<Palette>().is_err());
    }
    let mut value = Preset::default();
    for _ in 0..6 {
        value = value.next();
    }
    assert_eq!(value, Preset::Spectrum);
    assert_eq!(Preset::default().next(), Preset::Dusk);
}
#[test]
fn retained_fixed_raster_matches_endpoints_and_palette_solid() {
    let pixels = Preset::Dusk.rgba(Palette::Mocha);
    assert_eq!(pixels.len(), 640 * 400 * 4);
    assert_eq!(&pixels[..4], &[0x1e, 0x1e, 0x2e, 255]);
    assert_eq!(&pixels[pixels.len() - 4..], &[0xcb, 0xa6, 0xf7, 255]);
    assert!(
        Preset::Solid
            .rgba(Palette::Nord)
            .chunks_exact(4)
            .all(|p| p == [0x2e, 0x34, 0x40, 255])
    );
}

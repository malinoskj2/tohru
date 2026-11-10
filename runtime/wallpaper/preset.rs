//! Retained source attribution: j2bar, MIT; see J2BAR-LICENSE.
//! The six retained fixed640x400 raster presets. No paths, I/O or decoder.
use std::str::FromStr;

pub const WIDTH: u32 = 640;
pub const HEIGHT: u32 = 400;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Preset {
    #[default]
    Spectrum,
    Dusk,
    Tide,
    Ember,
    Moss,
    Solid,
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Palette {
    #[default]
    Mocha,
    Macchiato,
    RosePine,
    Nord,
    Gruvbox,
    TokyoNight,
    Dracula,
    Everforest,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
#[error("choose one of the six built-in wallpaper presets and eight retained palettes")]
pub struct Invalid;

impl Preset {
    pub const ALL: [Self; 6] = [
        Self::Spectrum,
        Self::Dusk,
        Self::Tide,
        Self::Ember,
        Self::Moss,
        Self::Solid,
    ];
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Spectrum => "spectrum",
            Self::Dusk => "dusk",
            Self::Tide => "tide",
            Self::Ember => "ember",
            Self::Moss => "moss",
            Self::Solid => "solid",
        }
    }
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Spectrum => Self::Dusk,
            Self::Dusk => Self::Tide,
            Self::Tide => Self::Ember,
            Self::Ember => Self::Moss,
            Self::Moss => Self::Solid,
            Self::Solid => Self::Spectrum,
        }
    }
    #[must_use]
    #[expect(clippy::unreadable_literal, reason = "hex colours read as #rrggbb")]
    pub const fn colors(self, palette: Palette) -> [u32; 3] {
        match self {
            Self::Spectrum => [0x11111b, 0x28233c, 0x4a3854],
            Self::Dusk => [0x1e1e2e, 0x45475a, 0xcba6f7],
            Self::Tide => [0x11111b, 0x1e3a4a, 0x89dceb],
            Self::Ember => [0x181825, 0x5a2a3a, 0xfab387],
            Self::Moss => [0x11111b, 0x2b3a2e, 0xa6e3a1],
            Self::Solid => [palette.base(); 3],
        }
    }
    /// Byte-rounded RGB sampling from j2bar's MIT-licensed fixed preset function.
    /// Fixed coordinates preserve its640x400 raster, including Centre/Tile fits.
    #[must_use]
    #[expect(
        clippy::cast_precision_loss,
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "640x400 coordinates are exact in f32 and blends stay within 0..=255"
    )]
    pub fn rgba(self, palette: Palette) -> Vec<u8> {
        let colors = self.colors(palette).map(|c| {
            let [_, r, g, b] = c.to_be_bytes();
            [r, g, b]
        });
        let mut pixels = Vec::with_capacity(WIDTH as usize * HEIGHT as usize * 4);
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                let t = ((y as f32 / (HEIGHT - 1) as f32) * 0.85
                    + (x as f32 / (WIDTH - 1) as f32) * 0.15)
                    .clamp(0.0, 1.0);
                let (start, end, f) = if t <= 0.55 {
                    (colors[0], colors[1], t / 0.55)
                } else {
                    (colors[1], colors[2], (t - 0.55) / 0.45)
                };
                for i in 0..3 {
                    pixels.push(
                        (f32::from(start[i]) * (1.0 - f) + f32::from(end[i]) * f).round() as u8,
                    );
                }
                pixels.push(255);
            }
        }
        pixels
    }
}
impl Palette {
    pub const ALL: [Self; 8] = [
        Self::Mocha,
        Self::Macchiato,
        Self::RosePine,
        Self::Nord,
        Self::Gruvbox,
        Self::TokyoNight,
        Self::Dracula,
        Self::Everforest,
    ];
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Mocha => "mocha",
            Self::Macchiato => "macchiato",
            Self::RosePine => "rose_pine",
            Self::Nord => "nord",
            Self::Gruvbox => "gruvbox",
            Self::TokyoNight => "tokyo_night",
            Self::Dracula => "dracula",
            Self::Everforest => "everforest",
        }
    }
    #[must_use]
    #[expect(clippy::unreadable_literal, reason = "hex colours read as #rrggbb")]
    pub const fn base(self) -> u32 {
        match self {
            Self::Mocha => 0x1e1e2e,
            Self::Macchiato => 0x24273a,
            Self::RosePine => 0x191724,
            Self::Nord => 0x2e3440,
            Self::Gruvbox => 0x282828,
            Self::TokyoNight => 0x1a1b26,
            Self::Dracula => 0x282a36,
            Self::Everforest => 0x2d353b,
        }
    }
}
impl FromStr for Preset {
    type Err = Invalid;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|v| v.name() == value)
            .ok_or(Invalid)
    }
}
impl FromStr for Palette {
    type Err = Invalid;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::ALL
            .into_iter()
            .find(|v| v.name() == value)
            .ok_or(Invalid)
    }
}

#[cfg(test)]
#[path = "../../tests/wallpaper/preset.rs"]
mod tests;

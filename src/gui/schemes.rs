//! The colour schemes the GUI shares with the TUI and with Rodeo and Interlinked.
//!
//! One table: a theme that exists on more than one surface has the same colours on
//! each. The terminal themes come from Rodeo's `src/themes.rs`, the editor themes
//! from Interlinked's registry (`shared-ui/themes.ts`), and IronMonitor's own from
//! its TUI palette. A dark theme's card surface is its background lifted 6% toward
//! its text; a light theme's panel is its sidebar grey with white cards.

use eframe::egui::Color32;

use super::app::ColorTheme;

/// What a theme paints with. Colours are `0xRRGGBB`.
#[derive(Debug, Clone, Copy)]
pub struct Scheme {
    /// Built on the light base rather than the dark one.
    pub light: bool,
    pub background: u32,
    /// Inputs and chart wells: the darkest surface (the lightest, on a light theme).
    pub dark: u32,
    /// Cards.
    pub surface: u32,
    pub foreground: u32,
    pub border: u32,
    pub accent: u32,
    pub secondary: u32,
    pub ok: u32,
    pub warn: u32,
    pub err: u32,
}

pub fn rgb(hex: u32) -> Color32 {
    Color32::from_rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

impl ColorTheme {
    /// The theme's scheme. `None` for the GUI's own accent-only themes, which
    /// recolour the Cyber base, and for Monochrome, which desaturates it.
    pub fn scheme(&self) -> Option<Scheme> {
        Some(match self {
            ColorTheme::IronMonitor => Scheme {
                light: false,
                background: 0x0f1318,
                dark: 0x0a0d11,
                surface: 0x161b22,
                foreground: 0xe6edf3,
                border: 0x2d323a,
                accent: 0x2ecc91,
                secondary: 0x5abedc,
                ok: 0x2ecc91,
                warn: 0xf0be50,
                err: 0xeb5f5f,
            },
            ColorTheme::Carbon => Scheme {
                light: false,
                background: 0x131313,
                dark: 0x0d0d0d,
                surface: 0x232323,
                foreground: 0xe6e6e6,
                border: 0x333333,
                accent: 0x7cb8ff,
                secondary: 0x9ccfd8,
                ok: 0x8fc78f,
                warn: 0xd9b26c,
                err: 0xe07a7a,
            },
            ColorTheme::Rodeo => Scheme {
                light: false,
                background: 0x050403,
                dark: 0x040302,
                surface: 0x301a12,
                foreground: 0xfff8f0,
                border: 0x4b3223,
                accent: 0xff6a00,
                secondary: 0xffaa00,
                ok: 0x64c896,
                warn: 0xffaa00,
                err: 0xff503c,
            },
            ColorTheme::Greyscale => Scheme {
                light: false,
                background: 0x121212,
                dark: 0x0d0d0d,
                surface: 0x262626,
                foreground: 0xf0f0f0,
                border: 0x3c3c3c,
                accent: 0xffffff,
                secondary: 0xc8c8c8,
                ok: 0xb4dcb4,
                warn: 0xdcc8a0,
                err: 0xdca0a0,
            },
            ColorTheme::AyuMirage => Scheme {
                light: false,
                background: 0x1f2335,
                dark: 0x161825,
                surface: 0x292e42,
                foreground: 0xcbccc6,
                border: 0x383e57,
                accent: 0xffad60,
                secondary: 0x73d8ff,
                ok: 0x87c38f,
                warn: 0xffd580,
                err: 0xff7575,
            },
            ColorTheme::OpenCode => Scheme {
                light: false,
                background: 0x1a1715,
                dark: 0x12100f,
                surface: 0x2b2623,
                foreground: 0xeeeeee,
                border: 0x38322e,
                accent: 0xfab283,
                secondary: 0xedb2f1,
                ok: 0x12c905,
                warn: 0xfcd53a,
                err: 0xfc533a,
            },
            ColorTheme::RosePine => Scheme {
                light: false,
                background: 0x191724,
                dark: 0x121019,
                surface: 0x21202e,
                foreground: 0xe0def4,
                border: 0x26233a,
                accent: 0xc4a7e7,
                secondary: 0x9ccfd8,
                ok: 0x9ccfd8,
                warn: 0xf6c177,
                err: 0xeb6f92,
            },
            ColorTheme::Kanagawa => Scheme {
                light: false,
                background: 0x1f1f28,
                dark: 0x16161c,
                surface: 0x2b2b33,
                foreground: 0xdcd7ba,
                border: 0x36363b,
                accent: 0x7fb4ca,
                secondary: 0x98bb6c,
                ok: 0x98bb6c,
                warn: 0xffa938,
                err: 0xc34043,
            },
            ColorTheme::CatppuccinMocha => Scheme {
                light: false,
                background: 0x1e1e2e,
                dark: 0x181825,
                surface: 0x28293a,
                foreground: 0xcdd6f4,
                border: 0x313244,
                accent: 0x89b4fa,
                secondary: 0x89b4fa,
                ok: 0xa6e3a1,
                warn: 0xf9e2af,
                err: 0xf38ba8,
            },
            ColorTheme::CatppuccinLatte => Scheme {
                light: true,
                background: 0xe6e9ef,
                dark: 0xeff1f5,
                surface: 0xeff1f5,
                foreground: 0x4c4f69,
                border: 0xccd0da,
                accent: 0x1e66f5,
                secondary: 0x8839ef,
                ok: 0x40a02b,
                warn: 0xdf8e1d,
                err: 0xd20f39,
            },
            ColorTheme::GlancesClassic => Scheme {
                light: false,
                background: 0x000000,
                dark: 0x000000,
                surface: 0x121212,
                foreground: 0xc0c0c0,
                border: 0x303030,
                accent: 0x00ffff,
                secondary: 0x00ff00,
                ok: 0x00ff00,
                warn: 0xffff00,
                err: 0xff0000,
            },
            ColorTheme::Nord => Scheme {
                light: false,
                background: 0x2e3440,
                dark: 0x272c36,
                surface: 0x393f4b,
                foreground: 0xeceff4,
                border: 0x3b4252,
                accent: 0x88c0d0,
                secondary: 0x88c0d0,
                ok: 0xa3be8c,
                warn: 0xebcb8b,
                err: 0xbf616a,
            },
            ColorTheme::Dracula => Scheme {
                light: false,
                background: 0x282a36,
                dark: 0x191a21,
                surface: 0x343641,
                foreground: 0xf8f8f2,
                border: 0x191a21,
                accent: 0xbd93f9,
                secondary: 0x8be9fd,
                ok: 0x50fa7b,
                warn: 0xf1fa8c,
                err: 0xff5555,
            },
            ColorTheme::GruvboxDark => Scheme {
                light: false,
                background: 0x282828,
                dark: 0x1d2021,
                surface: 0x343330,
                foreground: 0xebdbb2,
                border: 0x3c3836,
                accent: 0xfabd2f,
                secondary: 0x83a598,
                ok: 0xb8bb26,
                warn: 0xfabd2f,
                err: 0xfb4934,
            },
            ColorTheme::InterlinkedNervosys => Scheme {
                light: false,
                background: 0x131313,
                dark: 0x0f0f0f,
                surface: 0x212120,
                foreground: 0xf8f8f2,
                border: 0x2b2b2b,
                accent: 0xff14a4,
                secondary: 0x66d9ef,
                ok: 0xa9d75c,
                warn: 0xb75cff,
                err: 0xff3333,
            },
            ColorTheme::VsCodeDarkModern => Scheme {
                light: false,
                background: 0x1f1f1f,
                dark: 0x181818,
                surface: 0x292929,
                foreground: 0xcccccc,
                border: 0x2b2b2b,
                accent: 0x0078d4,
                secondary: 0x3794ff,
                ok: 0x81b88b,
                warn: 0xcca700,
                err: 0xf48771,
            },
            ColorTheme::VsCodeLightModern => Scheme {
                light: true,
                background: 0xf8f8f8,
                dark: 0xffffff,
                surface: 0xffffff,
                foreground: 0x3b3b3b,
                border: 0xe5e5e5,
                accent: 0x005fb8,
                secondary: 0x316bcd,
                ok: 0x587c0c,
                warn: 0xbf8803,
                err: 0xe51400,
            },
            ColorTheme::AyuMonokai => Scheme {
                light: false,
                background: 0x1f1f1f,
                dark: 0x1a1a1a,
                surface: 0x2c2c2c,
                foreground: 0xf8f8f2,
                border: 0x3a3a3a,
                accent: 0xff8f40,
                secondary: 0x66d9ef,
                ok: 0xa9d75c,
                warn: 0xffcc66,
                err: 0xff3333,
            },
            ColorTheme::SolarizedDark => Scheme {
                light: false,
                background: 0x002b36,
                dark: 0x001e26,
                surface: 0x09323c,
                foreground: 0x93a1a1,
                border: 0x586e75,
                accent: 0x268bd2,
                secondary: 0x2aa198,
                ok: 0x859900,
                warn: 0xb58900,
                err: 0xdc322f,
            },
            ColorTheme::OneDarkPro => Scheme {
                light: false,
                background: 0x282c34,
                dark: 0x21252b,
                surface: 0x30343c,
                foreground: 0xabb2bf,
                border: 0x181a1f,
                accent: 0x61afef,
                secondary: 0x61afef,
                ok: 0x98c379,
                warn: 0xe5c07b,
                err: 0xe06c75,
            },
            ColorTheme::GitHubDark => Scheme {
                light: false,
                background: 0x0d1117,
                dark: 0x010409,
                surface: 0x181d23,
                foreground: 0xc9d1d9,
                border: 0x30363d,
                accent: 0x58a6ff,
                secondary: 0x58a6ff,
                ok: 0x56d364,
                warn: 0xd29922,
                err: 0xf85149,
            },
            ColorTheme::GitHubLight => Scheme {
                light: true,
                background: 0xf6f8fa,
                dark: 0xffffff,
                surface: 0xffffff,
                foreground: 0x24292f,
                border: 0xd0d7de,
                accent: 0x0969da,
                secondary: 0x0969da,
                ok: 0x2da44e,
                warn: 0x9a6700,
                err: 0xcf222e,
            },
            ColorTheme::TokyoNight => Scheme {
                light: false,
                background: 0x1a1b26,
                dark: 0x16161e,
                surface: 0x242632,
                foreground: 0xc0caf5,
                border: 0x15161e,
                accent: 0x7aa2f7,
                secondary: 0x7dcfff,
                ok: 0x9ece6a,
                warn: 0xe0af68,
                err: 0xf7768e,
            },
            ColorTheme::Monokai => Scheme {
                light: false,
                background: 0x272822,
                dark: 0x1e1f1c,
                surface: 0x34342e,
                foreground: 0xf8f8f2,
                border: 0x3e3d32,
                accent: 0xa6e22e,
                secondary: 0x66d9ef,
                ok: 0xa6e22e,
                warn: 0xe6db74,
                err: 0xf92672,
            },
            ColorTheme::NightOwl => Scheme {
                light: false,
                background: 0x011627,
                dark: 0x011221,
                surface: 0x0e2233,
                foreground: 0xd6deeb,
                border: 0x1d3b53,
                accent: 0x82aaff,
                secondary: 0x82aaff,
                ok: 0xaddb67,
                warn: 0xecc48d,
                err: 0xef5350,
            },
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every theme Interlinked's registry (Rodeo `shared-ui/themes.ts`) ships.
    const INTERLINKED: &[&str] = &[
        "Interlinked Nervosys",
        "Monochrome",
        "VS Code Dark Modern",
        "VS Code Light Modern",
        "Ayu Monokai",
        "Solarized Dark",
        "One Dark Pro",
        "Dracula",
        "GitHub Dark",
        "GitHub Light",
        "Tokyo Night",
        "Nord",
        "Gruvbox Dark",
        "Catppuccin Mocha",
        "Monokai",
        "Night Owl",
    ];

    fn names() -> Vec<&'static str> {
        ColorTheme::all().iter().map(|t| t.name()).collect()
    }

    #[test]
    fn the_gui_offers_every_interlinked_theme() {
        let gui = names();
        for name in INTERLINKED {
            assert!(gui.contains(name), "GUI lacks Interlinked's {name}");
        }
    }

    /// The TUI carries Rodeo's terminal themes, so this covers those too.
    #[cfg(feature = "cli")]
    #[test]
    fn the_gui_offers_every_tui_theme_in_the_same_colours() {
        use crate::tui::app::ColorTheme as Tui;
        let gui = ColorTheme::all();
        for tui in Tui::all() {
            let theme = gui
                .iter()
                .find(|g| g.name() == tui.name())
                .unwrap_or_else(|| panic!("GUI lacks the TUI's {}", tui.name()));
            if let Some(scheme) = theme.scheme() {
                let ratatui::style::Color::Rgb(r, g, b) = tui.palette().accent else {
                    continue;
                };
                assert_eq!(
                    rgb(scheme.accent),
                    Color32::from_rgb(r, g, b),
                    "{}: the GUI and TUI accents differ",
                    tui.name()
                );
            }
        }
    }

    #[test]
    fn theme_names_are_unique() {
        let mut seen = names();
        seen.sort_unstable();
        let len = seen.len();
        seen.dedup();
        assert_eq!(seen.len(), len, "two themes share a name");
    }
}

use ratatui::style::Color;
use serde::Deserialize;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ThemeName {
    #[default]
    Dark,
    Light,
    Ocean,
    Forest,
}

impl ThemeName {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Dark => "Dark",
            Self::Light => "Light",
            Self::Ocean => "Ocean",
            Self::Forest => "Forest",
        }
    }

    pub const fn next(self) -> Self {
        match self {
            Self::Dark => Self::Light,
            Self::Light => Self::Ocean,
            Self::Ocean => Self::Forest,
            Self::Forest => Self::Dark,
        }
    }

    pub const fn palette(self) -> Theme {
        match self {
            Self::Dark => Theme {
                accent: Color::Rgb(124, 134, 255),
                fg: Color::Reset,
                muted: Color::Gray,
                quiet: Color::DarkGray,
                border: Color::DarkGray,
                select_fg: Color::White,
                select_bg: Color::Rgb(54, 58, 108),
                success: Color::LightGreen,
                warn: Color::LightYellow,
                danger: Color::LightRed,
                keyword: Color::Rgb(158, 168, 255),
                string: Color::LightGreen,
                comment: Color::DarkGray,
                number: Color::LightCyan,
            },
            Self::Light => Theme {
                accent: Color::Rgb(58, 67, 186),
                fg: Color::Black,
                muted: Color::Rgb(75, 85, 99),
                quiet: Color::Rgb(107, 114, 128),
                border: Color::Rgb(156, 163, 175),
                select_fg: Color::White,
                select_bg: Color::Rgb(58, 67, 186),
                success: Color::Rgb(21, 128, 61),
                warn: Color::Rgb(161, 98, 7),
                danger: Color::Rgb(185, 28, 28),
                keyword: Color::Rgb(58, 67, 186),
                string: Color::Rgb(21, 128, 61),
                comment: Color::Rgb(107, 114, 128),
                number: Color::Rgb(15, 118, 110),
            },
            Self::Ocean => Theme {
                accent: Color::Cyan,
                fg: Color::Reset,
                muted: Color::Gray,
                quiet: Color::DarkGray,
                border: Color::DarkGray,
                select_fg: Color::Black,
                select_bg: Color::Cyan,
                success: Color::LightGreen,
                warn: Color::LightYellow,
                danger: Color::LightRed,
                keyword: Color::LightCyan,
                string: Color::LightGreen,
                comment: Color::DarkGray,
                number: Color::LightYellow,
            },
            Self::Forest => Theme {
                accent: Color::LightGreen,
                fg: Color::Reset,
                muted: Color::Gray,
                quiet: Color::DarkGray,
                border: Color::DarkGray,
                select_fg: Color::Black,
                select_bg: Color::LightGreen,
                success: Color::LightGreen,
                warn: Color::LightYellow,
                danger: Color::LightRed,
                keyword: Color::LightGreen,
                string: Color::LightYellow,
                comment: Color::DarkGray,
                number: Color::LightCyan,
            },
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Theme {
    pub accent: Color,
    pub fg: Color,
    pub muted: Color,
    pub quiet: Color,
    pub border: Color,
    pub select_fg: Color,
    pub select_bg: Color,
    pub success: Color,
    pub warn: Color,
    pub danger: Color,
    pub keyword: Color,
    pub string: Color,
    pub comment: Color,
    pub number: Color,
}

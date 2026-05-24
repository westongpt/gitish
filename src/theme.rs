use ratatui::style::Color;
use serde::Deserialize;
use std::path::Path;

use crate::error::AppError;

#[allow(dead_code)]
pub struct Theme {
    pub base00: Color,
    pub base01: Color,
    pub base02: Color,
    pub base03: Color,
    pub base04: Color,
    pub base05: Color,
    pub base06: Color,
    pub base07: Color,
    pub base08: Color,
    pub base09: Color,
    pub base0a: Color,
    pub base0b: Color,
    pub base0c: Color,
    pub base0d: Color,
    pub base0e: Color,
    pub base0f: Color,
}

pub struct NamedTheme {
    pub name: String,
    pub theme: Theme,
}

#[derive(Deserialize)]
struct ThemeFile {
    scheme: String,
    base00: String,
    base01: String,
    base02: String,
    base03: String,
    base04: String,
    base05: String,
    base06: String,
    base07: String,
    base08: String,
    base09: String,
    base0a: String,
    base0b: String,
    base0c: String,
    base0d: String,
    base0e: String,
    base0f: String,
}

impl ThemeFile {
    fn into_named_theme(self) -> Option<NamedTheme> {
        Some(NamedTheme {
            name: self.scheme,
            theme: Theme {
                base00: parse_hex(&self.base00)?,
                base01: parse_hex(&self.base01)?,
                base02: parse_hex(&self.base02)?,
                base03: parse_hex(&self.base03)?,
                base04: parse_hex(&self.base04)?,
                base05: parse_hex(&self.base05)?,
                base06: parse_hex(&self.base06)?,
                base07: parse_hex(&self.base07)?,
                base08: parse_hex(&self.base08)?,
                base09: parse_hex(&self.base09)?,
                base0a: parse_hex(&self.base0a)?,
                base0b: parse_hex(&self.base0b)?,
                base0c: parse_hex(&self.base0c)?,
                base0d: parse_hex(&self.base0d)?,
                base0e: parse_hex(&self.base0e)?,
                base0f: parse_hex(&self.base0f)?,
            },
        })
    }
}

fn parse_hex(s: &str) -> Option<Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    let v = u32::from_str_radix(s, 16).ok()?;
    Some(Color::Rgb(
        ((v >> 16) & 0xFF) as u8,
        ((v >> 8) & 0xFF) as u8,
        (v & 0xFF) as u8,
    ))
}

pub fn fallback_theme() -> NamedTheme {
    NamedTheme {
        name: "Terminal Default".into(),
        theme: Theme {
            base00: Color::Reset,
            base01: Color::Reset,
            base02: Color::DarkGray,
            base03: Color::DarkGray,
            base04: Color::Gray,
            base05: Color::White,
            base06: Color::White,
            base07: Color::White,
            base08: Color::Red,
            base09: Color::Yellow,
            base0a: Color::Yellow,
            base0b: Color::Green,
            base0c: Color::Cyan,
            base0d: Color::Blue,
            base0e: Color::Magenta,
            base0f: Color::Red,
        },
    }
}

pub fn all_themes(config_dir: &Path) -> Vec<NamedTheme> {
    let themes_dir = config_dir.join("themes");
    let Ok(entries) = std::fs::read_dir(&themes_dir) else {
        return vec![fallback_theme()];
    };

    let mut paths: Vec<_> = entries
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    paths.sort();

    let mut themes = Vec::new();
    for path in paths {
        let Ok(content) = std::fs::read_to_string(&path) else {
            continue;
        };
        match serde_yaml_ng::from_str::<ThemeFile>(&content) {
            Ok(tf) => {
                if let Some(nt) = tf.into_named_theme() {
                    themes.push(nt);
                }
            }
            Err(e) => eprintln!("warning: skipping {}: {e}", path.display()),
        }
    }

    if themes.is_empty() {
        vec![fallback_theme()]
    } else {
        themes
    }
}

pub fn load_theme_by_name<'a>(
    themes: &'a [NamedTheme],
    name: &str,
) -> Option<&'a NamedTheme> {
    themes
        .iter()
        .find(|t| t.name.eq_ignore_ascii_case(name))
}

pub fn seed_themes(config_dir: &Path) -> Result<(), AppError> {
    let themes_dir = config_dir.join("themes");
    std::fs::create_dir_all(&themes_dir)?;
    for (filename, content) in crate::seeds::DEFAULT_THEMES {
        let dest = themes_dir.join(filename);
        if !dest.exists() {
            std::fs::write(dest, content)?;
        }
    }
    Ok(())
}

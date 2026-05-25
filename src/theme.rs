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

/// A non-empty list of themes with a tracked current index.
///
/// The non-empty invariant is enforced at construction: if the supplied `Vec`
/// is empty the fallback theme is inserted so that indexing is always safe.
pub struct ThemeList {
    themes: Vec<NamedTheme>,
    current_idx: usize,
}

impl ThemeList {
    pub fn new(mut themes: Vec<NamedTheme>) -> Self {
        if themes.is_empty() {
            themes.push(fallback_theme());
        }
        ThemeList { themes, current_idx: 0 }
    }

    pub fn current(&self) -> &NamedTheme {
        &self.themes[self.current_idx]
    }

    pub fn current_idx(&self) -> usize {
        self.current_idx
    }

    pub fn set_current_idx(&mut self, idx: usize) {
        if idx < self.themes.len() {
            self.current_idx = idx;
        }
    }

    pub fn find_idx_by_name(&self, name: &str) -> Option<usize> {
        self.themes.iter().position(|t| t.name.eq_ignore_ascii_case(name))
    }

    pub fn len(&self) -> usize {
        self.themes.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = &NamedTheme> {
        self.themes.iter()
    }

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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn parse_hex_valid_lowercase() {
        assert_eq!(parse_hex("1e1e2e"), Some(Color::Rgb(0x1e, 0x1e, 0x2e)));
    }

    #[test]
    fn parse_hex_with_leading_hash() {
        assert_eq!(parse_hex("#cdd6f4"), Some(Color::Rgb(0xcd, 0xd6, 0xf4)));
    }

    #[test]
    fn parse_hex_all_zeros() {
        assert_eq!(parse_hex("000000"), Some(Color::Rgb(0, 0, 0)));
    }

    #[test]
    fn parse_hex_all_ff() {
        assert_eq!(parse_hex("ffffff"), Some(Color::Rgb(255, 255, 255)));
    }

    #[test]
    fn parse_hex_too_short_returns_none() {
        assert!(parse_hex("ff00").is_none());
    }

    #[test]
    fn parse_hex_too_long_returns_none() {
        assert!(parse_hex("ff0000ff").is_none());
    }

    #[test]
    fn parse_hex_invalid_chars_returns_none() {
        assert!(parse_hex("gggggg").is_none());
    }

    #[test]
    fn fallback_theme_name_is_terminal_default() {
        assert_eq!(fallback_theme().name, "Terminal Default");
    }

    #[test]
    fn find_idx_by_name_exact_match() {
        let list = ThemeList::new(vec![NamedTheme {
            name: "Catppuccin Mocha".into(),
            theme: fallback_theme().theme,
        }]);
        assert_eq!(list.find_idx_by_name("Catppuccin Mocha"), Some(0));
    }

    #[test]
    fn find_idx_by_name_case_insensitive() {
        let list = ThemeList::new(vec![NamedTheme {
            name: "Catppuccin Mocha".into(),
            theme: fallback_theme().theme,
        }]);
        assert!(list.find_idx_by_name("catppuccin mocha").is_some());
        assert!(list.find_idx_by_name("CATPPUCCIN MOCHA").is_some());
    }

    #[test]
    fn find_idx_by_name_not_found_returns_none() {
        let list = ThemeList::new(vec![NamedTheme {
            name: "Foo".into(),
            theme: fallback_theme().theme,
        }]);
        assert!(list.find_idx_by_name("Bar").is_none());
    }

    #[test]
    fn theme_list_new_with_empty_vec_inserts_fallback() {
        let list = ThemeList::new(vec![]);
        assert_eq!(list.len(), 1);
        assert_eq!(list.current().name, "Terminal Default");
    }

    #[test]
    fn theme_list_set_current_idx_clamps_to_valid_range() {
        let list = ThemeList::new(vec![NamedTheme {
            name: "A".into(),
            theme: fallback_theme().theme,
        }]);
        let mut list = list;
        list.set_current_idx(99);
        assert_eq!(list.current_idx(), 0, "out-of-bounds idx must not be applied");
    }

    #[test]
    fn theme_list_current_returns_initial_theme() {
        let list = ThemeList::new(vec![NamedTheme {
            name: "Only".into(),
            theme: fallback_theme().theme,
        }]);
        assert_eq!(list.current().name, "Only");
    }

    #[test]
    fn all_themes_returns_fallback_when_no_themes_dir() {
        let dir = TempDir::new().unwrap();
        let themes = all_themes(dir.path());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "Terminal Default");
    }

    #[test]
    fn all_themes_returns_fallback_when_themes_dir_is_empty() {
        let dir = TempDir::new().unwrap();
        std::fs::create_dir(dir.path().join("themes")).unwrap();
        let themes = all_themes(dir.path());
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "Terminal Default");
    }

    #[test]
    fn seed_themes_creates_at_least_one_yaml() {
        let dir = TempDir::new().unwrap();
        seed_themes(dir.path()).unwrap();
        let themes_dir = dir.path().join("themes");
        let count = std::fs::read_dir(&themes_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .is_some_and(|x| x == "yaml" || x == "yml")
            })
            .count();
        assert!(count > 0, "seed_themes must write at least one theme file");
    }

    #[test]
    fn all_themes_after_seed_returns_named_themes() {
        let dir = TempDir::new().unwrap();
        seed_themes(dir.path()).unwrap();
        let themes = all_themes(dir.path());
        assert!(
            themes.iter().any(|t| t.name != "Terminal Default"),
            "seeded themes should include at least one non-fallback theme"
        );
    }

    #[test]
    fn seed_themes_is_idempotent() {
        let dir = TempDir::new().unwrap();
        seed_themes(dir.path()).unwrap();
        let count_first = std::fs::read_dir(dir.path().join("themes"))
            .unwrap()
            .count();
        seed_themes(dir.path()).unwrap();
        let count_second = std::fs::read_dir(dir.path().join("themes"))
            .unwrap()
            .count();
        assert_eq!(count_first, count_second, "seeding twice must not duplicate files");
    }
}

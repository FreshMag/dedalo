//! The registry: the one place built-ins, packs and plugins enter through
//! (spec section 11).
//!
//! Release 0.1 holds only the built-ins, but they register through the same
//! calls third parties will get: the catalog and the `classic` theme are
//! pack files read by the ordinary loaders, and the built-in painters go
//! through the painter registry. The pack loader for third-party packages
//! and the WebAssembly host arrive with milestone M6.

use std::collections::BTreeMap;

use dedalo_catalog::Catalog;
use dedalo_render::{Painters, Theme};

/// The catalog, themes and painters in effect.
pub struct Registry {
    pub catalog: Catalog,
    pub themes: BTreeMap<String, Theme>,
    pub painters: Painters,
    /// IDs of the plugins that are loaded. Empty until M6.
    pub plugins: Vec<String>,
}

/// The theme used when a document names none.
pub const DEFAULT_THEME: &str = "classic";

impl Registry {
    /// The registry of a tool with no third-party extensions.
    pub fn builtin() -> Registry {
        let mut registry = Registry {
            catalog: Catalog::builtin(),
            themes: BTreeMap::new(),
            painters: Painters::builtin(),
            plugins: Vec::new(),
        };
        registry.add_theme(Theme::classic());
        registry
    }

    pub fn add_theme(&mut self, theme: Theme) {
        self.themes.insert(theme.id().to_string(), theme);
    }

    /// The theme with this name, or the default theme when none is asked for.
    pub fn theme(&self, name: Option<&str>) -> Option<&Theme> {
        self.themes.get(name.unwrap_or(DEFAULT_THEME))
    }
}

impl Default for Registry {
    fn default() -> Self {
        Registry::builtin()
    }
}

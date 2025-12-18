//! Built-in prompt regex presets for common shells

/// A preset prompt pattern
pub struct Preset {
    pub name: &'static str,
    pub regex: &'static str,
    pub description: &'static str,
}

/// Available presets for common shell configurations
pub const PRESETS: &[Preset] = &[
    Preset {
        name: "simple",
        regex: r"^\$ ",
        description: "Simple bash prompt ($ )",
    },
    Preset {
        name: "zsh",
        regex: r"^.*% ",
        description: "Default zsh prompt",
    },
    Preset {
        name: "oh-my-zsh",
        regex: r"^.*➜ ",
        description: "Oh My Zsh default theme",
    },
    Preset {
        name: "starship",
        regex: r"^.*[❯➜] ",
        description: "Starship cross-shell prompt",
    },
    Preset {
        name: "fish",
        regex: r"^.*> ",
        description: "Fish default prompt",
    },
];

/// Get a preset regex by name
pub fn get_by_name(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

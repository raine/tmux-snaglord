//! Built-in prompt regex presets for common shells

/// A preset prompt pattern
pub struct Preset {
    pub name: &'static str,
    pub regex: &'static str,
    pub description: &'static str,
}

/// Available presets for common shell configurations
pub const PRESETS: &[Preset] = &[
    // Standard: user@host:~/path$
    Preset {
        name: "bash",
        regex: r"^[\w.-]+@[\w.-]+:[~\w./-]+[#$] ",
        description: "Standard bash (user@host:path$)",
    },
    // Default zsh: hostname%
    Preset {
        name: "zsh",
        regex: r"^[\w.-]+% ",
        description: "Default zsh (hostname%)",
    },
    // Fish default: path>
    Preset {
        name: "fish",
        regex: r"^.*?[\w./-]+> ",
        description: "Fish default prompt",
    },
    // Oh My Zsh robbyrussell theme: ➜  dir
    Preset {
        name: "robbyrussell",
        regex: r"^➜  ",
        description: "Oh My Zsh robbyrussell theme",
    },
    // Starship default symbol. `❯` followed by space *or* end-of-line so
    // bare prompt redraws (where tmux has stripped trailing space) still
    // match.
    Preset {
        name: "starship",
        regex: r"^❯(?: |$)",
        description: "Starship default prompt",
    },
    // Simple fallbacks
    Preset {
        name: "dollar",
        regex: r"^\$ ",
        description: "Simple $ prompt",
    },
    Preset {
        name: "hash",
        regex: r"^# ",
        description: "Root shell prompt",
    },
];

/// Get a preset regex by name
pub fn get_by_name(name: &str) -> Option<&'static Preset> {
    PRESETS.iter().find(|p| p.name == name)
}

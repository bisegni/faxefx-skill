//! Internal presentation registry. No MIDI, agent, or persistence dependencies.
use std::collections::BTreeMap;
pub struct Presentation {
    pub icon: &'static str,
    pub accent: iced::Color,
}
pub struct EffectWidgetRegistry {
    families: BTreeMap<&'static str, Presentation>,
    unknown: Presentation,
}
impl Default for EffectWidgetRegistry {
    fn default() -> Self {
        Self {
            families: BTreeMap::from([
                (
                    "amp",
                    Presentation {
                        icon: "Amp",
                        accent: iced::color!(0xe9bb41),
                    },
                ),
                (
                    "input",
                    Presentation {
                        icon: "Input",
                        accent: iced::color!(0x4aaff0),
                    },
                ),
                (
                    "output",
                    Presentation {
                        icon: "Output",
                        accent: iced::color!(0x4aaff0),
                    },
                ),
            ]),
            unknown: Presentation {
                icon: "?",
                accent: iced::color!(0x879aa9),
            },
        }
    }
}
impl EffectWidgetRegistry {
    pub fn presentation(&self, family: &str) -> &Presentation {
        self.families.get(family).unwrap_or(&self.unknown)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn unknown_is_generic() {
        assert_eq!(
            EffectWidgetRegistry::default()
                .presentation("unrecovered")
                .icon,
            "?"
        );
    }
}

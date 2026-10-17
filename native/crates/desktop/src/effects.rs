//! Internal presentation registry. No MIDI, agent, or persistence dependencies.
use std::collections::BTreeMap;
pub struct Presentation {
    pub icon: &'static str,
}
pub struct EffectWidgetRegistry {
    families: BTreeMap<&'static str, Presentation>,
    unknown: Presentation,
}
impl Default for EffectWidgetRegistry {
    fn default() -> Self {
        Self {
            families: BTreeMap::from([
                ("amp", Presentation { icon: "AMP" }),
                ("input", Presentation { icon: "IN" }),
                ("output", Presentation { icon: "OUT" }),
            ]),
            unknown: Presentation { icon: "?" },
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

//! Effect-family presentation registry. No MIDI, agent, or persistence dependencies.
use std::collections::BTreeMap;

pub struct Presentation {
    /// Short family label shown beside the vector glyph in the inspector.
    pub icon: &'static str,
    pub accent: iced::Color,
}

pub struct EffectWidgetRegistry {
    families: BTreeMap<&'static str, Presentation>,
    unknown: Presentation,
}

impl Default for EffectWidgetRegistry {
    fn default() -> Self {
        let mut families = BTreeMap::new();
        let mut add = |accent, entries: &[(&'static str, &'static str)]| {
            for (family, label) in entries {
                families.insert(
                    *family,
                    Presentation {
                        icon: label,
                        accent,
                    },
                );
            }
        };

        add(iced::color!(0xe9bb41), &[("amp", "Amp")]);
        add(
            iced::color!(0x55b9d5),
            &[("cab", "Cab"), ("ir-player", "IR"), ("nam-player", "NAM")],
        );
        add(
            iced::color!(0xe98559),
            &[
                ("drive", "Drive"),
                ("dynamic-distortion", "DynaDist"),
                ("ringmod", "Ring Mod"),
            ],
        );
        add(
            iced::color!(0xdfa34f),
            &[
                ("comp", "Comp"),
                ("multicomp", "MultiComp"),
                ("gate", "Gate"),
            ],
        );
        add(
            iced::color!(0x9b86e8),
            &[
                ("delay", "Delay"),
                ("multitap", "MultiTap"),
                ("megatap", "MegaTap"),
                ("tentap", "TenTap"),
                ("reverb", "Reverb"),
                ("plex", "Plex"),
                ("resonator", "Resonator"),
                ("feedback-send", "Feedback Send"),
                ("feedback-return", "Feedback Return"),
            ],
        );
        add(
            iced::color!(0xd574bd),
            &[
                ("chorus", "Chorus"),
                ("flanger", "Flanger"),
                ("phaser", "Phaser"),
                ("rotary", "Rotary"),
                ("tremolo", "Tremolo"),
            ],
        );
        add(
            iced::color!(0x55b9a0),
            &[
                ("geq", "Graphic EQ"),
                ("peq", "Parametric EQ"),
                ("filter", "Filter"),
                ("wah", "Wah"),
                ("formant", "Formant"),
                ("crossover", "Crossover"),
                ("enhancer", "Enhancer"),
                ("tone-match", "Tone Match"),
            ],
        );
        add(
            iced::color!(0x4aaff0),
            &[
                ("input", "Input"),
                ("output", "Output"),
                ("mixer", "Mixer"),
                ("multiplexer", "Multiplexer"),
                ("volume", "Volume"),
                ("pitch", "Pitch"),
                ("synth", "Synth"),
                ("vocoder", "Vocoder"),
                ("looper", "Looper"),
                ("midiblock", "MIDI"),
                ("footcontroller", "Foot Ctrl"),
                ("preset-fc", "Preset FC"),
                ("perform", "Perform"),
                ("tuner", "Tuner"),
                ("ircapture", "IR Capture"),
                ("rta", "Analyzer"),
            ],
        );

        Self {
            families,
            unknown: Presentation {
                icon: "Unknown",
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
    fn every_bundled_effect_family_has_a_presentation() {
        let registry = EffectWidgetRegistry::default();
        let catalog: serde_json::Value = serde_json::from_str(include_str!(
            "../../../resources/axe-effects/identities.json"
        ))
        .unwrap();
        let families = catalog["instances"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|item| item["family"].as_str())
            .collect::<std::collections::BTreeSet<_>>();

        assert!(families.len() > 40);
        for family in families {
            assert_ne!(registry.presentation(family).icon, "Unknown", "{family}");
        }
    }

    #[test]
    fn truly_unknown_families_keep_the_generic_presentation() {
        assert_eq!(
            EffectWidgetRegistry::default()
                .presentation("unrecovered")
                .icon,
            "Unknown"
        );
    }
}

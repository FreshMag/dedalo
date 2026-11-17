//! The words a map prints by itself, in each supported language: legend
//! labels of the built-in types, panel headings and the scale line.
//!
//! English comes from the catalog. A document picks a language with
//! `render { language = it }`; a legend `entry` block still overrides any
//! single label.

use dedalo_model::Language;

/// Italian, by legend key: a connection kind, `type` or `type.subtype`.
const ITALIAN: &[(&str, &str)] = &[
    ("legend", "Legenda"),
    ("notes", "Note"),
    ("square", "quadretto"),
    ("door", "Porta"),
    ("locked_door", "Porta chiusa a chiave"),
    ("secret_door", "Porta segreta"),
    ("archway", "Arco"),
    ("stairs", "Scale"),
    ("ladder", "Scala a pioli"),
    ("trapdoor", "Botola"),
    ("architecture", "Colonna"),
    ("architecture.column", "Colonna"),
    ("architecture.pillar", "Pilastro"),
    ("furniture", "Arredo"),
    ("furniture.table", "Tavolo"),
    ("furniture.chair", "Sedia"),
    ("furniture.wardrobe", "Armadio"),
    ("furniture.altar", "Altare"),
    ("furniture.statue", "Statua"),
    ("furniture.sarcophagus", "Sarcofago"),
    ("furniture.crate", "Cassa"),
    ("mechanism", "Meccanismo"),
    ("mechanism.lever", "Leva"),
    ("mechanism.button", "Pulsante"),
    ("mechanism.generic", "Meccanismo"),
    ("trap", "Trappola"),
    ("trap.pressure_plate", "Piastra a pressione"),
    ("trap.dart_launcher", "Lanciadardi"),
    ("trap.pit", "Fossa"),
    ("trap.generic", "Trappola"),
    ("lighting", "Luce"),
    ("lighting.torch", "Torcia"),
    ("lighting.brazier", "Braciere"),
    ("lighting.candle", "Candela"),
    ("lighting.glowing_crystal", "Cristallo luminoso"),
    ("terrain", "Terreno"),
    ("terrain.water", "Acqua"),
    ("terrain.lava", "Lava"),
    ("terrain.rubble", "Macerie"),
    ("terrain.rocks", "Rocce"),
    ("container", "Forziere"),
    ("container.chest", "Forziere"),
    ("creature", "Creatura"),
    ("npc", "PNG"),
    ("item", "Oggetto"),
    ("item.treasure", "Tesoro"),
    ("item.key", "Chiave"),
    ("item.weapon", "Arma"),
    ("item.armor", "Armatura"),
    ("item.scroll", "Pergamena"),
    ("item.generic", "Oggetto"),
    ("custom", "Personalizzato"),
];

/// The word for a key in a language. Nothing for English, whose words come
/// from the catalog, and for keys the table does not know, such as the
/// types of a plugin.
pub fn word(language: Language, key: &str) -> Option<&'static str> {
    match language {
        Language::En => None,
        Language::It => ITALIAN.iter().find(|(k, _)| *k == key).map(|(_, w)| *w),
    }
}

/// The word for a key, or the English given.
pub fn word_or<'a>(language: Language, key: &str, english: &'a str) -> &'a str {
    word(language, key).unwrap_or(english)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_built_in_type_and_subtype_has_an_italian_label() {
        let catalog = dedalo_catalog::Catalog::builtin();
        for (name, entry) in &catalog.types {
            assert!(word(Language::It, name).is_some(), "{name}");
            for subtype in &entry.subtypes {
                let key = format!("{name}.{subtype}");
                assert!(word(Language::It, &key).is_some(), "{key}");
            }
        }
        assert_eq!(word(Language::En, "door"), None);
        assert_eq!(
            word_or(Language::It, "blueprint.console", "Console"),
            "Console"
        );
    }
}

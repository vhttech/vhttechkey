//! Load `vietnamese.cm.dict` — embedded at compile time, with optional override via env var.

use std::io::Cursor;
use std::sync::Arc;

use tracing::{info, warn};
use vi_config::Config;
use vi_core::{SpellOptions, VietnameseDict};

/// The dictionary bundled into the binary at compile time.
static EMBEDDED_DICT: &[u8] =
    include_bytes!("../../../data/dictionaries/vietnamese.cm.dict");

/// Load the dictionary.
///
/// If `VIME_VIETNAMESE_DICT` points to an existing file, that file is used instead
/// of the embedded copy (useful for testing custom word lists).
/// Falls back to the embedded dictionary when the env var is absent or invalid.
pub fn load_dictionary_arc() -> Arc<VietnameseDict> {
    if let Ok(p) = std::env::var("VIME_VIETNAMESE_DICT") {
        let path = std::path::Path::new(p.trim());
        if path.is_file() {
            match VietnameseDict::load_from_path(path) {
                Ok(d) => {
                    info!(
                        "Loaded Vietnamese dictionary from {} ({} entries)",
                        path.display(),
                        d.len()
                    );
                    return Arc::new(d);
                }
                Err(e) => {
                    warn!(
                        "VIME_VIETNAMESE_DICT={} could not be read ({e}); falling back to embedded dict",
                        path.display()
                    );
                }
            }
        } else {
            warn!(
                "VIME_VIETNAMESE_DICT={} does not exist; falling back to embedded dict",
                p.trim()
            );
        }
    }

    let dict = VietnameseDict::load_from_reader(Cursor::new(EMBEDDED_DICT))
        .expect("embedded dictionary is always valid UTF-8");
    info!("Loaded embedded Vietnamese dictionary ({} entries)", dict.len());
    Arc::new(dict)
}

/// Build [`SpellOptions`] from the active profile and an optionally loaded dictionary.
pub fn spell_options_from_config(cfg: &Config, dictionary: Option<Arc<VietnameseDict>>) -> SpellOptions {
    let sc = cfg
        .profiles
        .get(&cfg.active_profile)
        .map(|p| p.spell_check.clone())
        .unwrap_or_default();

    if !sc.enabled || !sc.commit_with_dictionary {
        return SpellOptions {
            dictionary: None,
            commit_spell_check_dict: false,
            dd_freestyle: sc.dd_freestyle,
        };
    }

    match dictionary {
        Some(dict) => SpellOptions {
            dictionary: Some(dict),
            commit_spell_check_dict: true,
            dd_freestyle: sc.dd_freestyle,
        },
        None => {
            warn!(
                profile = %cfg.active_profile,
                "spell_check.commit_with_dictionary is enabled but no dictionary file was found; continuing without dict lookup"
            );
            SpellOptions {
                dictionary: None,
                commit_spell_check_dict: false,
                dd_freestyle: sc.dd_freestyle,
            }
        }
    }
}

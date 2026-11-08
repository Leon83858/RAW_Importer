//! Central file-format registry.
//!
//! This is the only place that knows which extensions belong to which format.
//! The frontend receives the registry from the backend instead of keeping its
//! own copy (master prompt §8).

use std::collections::{BTreeSet, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub enum FormatId {
    Arw,
    Cr2,
    Cr3,
    Nef,
    Raf,
    Orf,
    Rw2,
    Dng,
    Jpg,
    Jpeg,
    Png,
    Tiff,
    Heic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FormatCategory {
    Raw,
    Image,
}

#[derive(Debug, Clone, Serialize)]
pub struct FormatInfo {
    pub id: FormatId,
    pub category: FormatCategory,
    /// Lower-case extensions without the leading dot.
    pub extensions: &'static [&'static str],
    /// Camera vendor for RAW formats, informational only.
    pub vendor: Option<&'static str>,
}

/// All formats supported by RAW Importer 2.x, in the 2.x UI order.
pub const REGISTRY: &[FormatInfo] = &[
    raw(FormatId::Arw, &["arw"], "Sony"),
    raw(FormatId::Cr2, &["cr2"], "Canon"),
    raw(FormatId::Cr3, &["cr3"], "Canon"),
    raw(FormatId::Nef, &["nef"], "Nikon"),
    raw(FormatId::Raf, &["raf"], "Fujifilm"),
    raw(FormatId::Orf, &["orf"], "OM System / Olympus"),
    raw(FormatId::Rw2, &["rw2"], "Panasonic"),
    raw(FormatId::Dng, &["dng"], "Adobe"),
    image(FormatId::Jpg, &["jpg"]),
    image(FormatId::Jpeg, &["jpeg"]),
    image(FormatId::Png, &["png"]),
    // 2.x: the TIFF toggle includes .tif and the HEIC toggle includes .heif.
    image(FormatId::Tiff, &["tiff", "tif"]),
    image(FormatId::Heic, &["heic", "heif"]),
];

const fn raw(
    id: FormatId,
    extensions: &'static [&'static str],
    vendor: &'static str,
) -> FormatInfo {
    FormatInfo {
        id,
        category: FormatCategory::Raw,
        extensions,
        vendor: Some(vendor),
    }
}

const fn image(id: FormatId, extensions: &'static [&'static str]) -> FormatInfo {
    FormatInfo {
        id,
        category: FormatCategory::Image,
        extensions,
        vendor: None,
    }
}

pub fn info(id: FormatId) -> &'static FormatInfo {
    REGISTRY
        .iter()
        .find(|f| f.id == id)
        .expect("every FormatId is in REGISTRY")
}

/// Maximum length of a manual extension (without dot).
pub const MAX_MANUAL_EXTENSION_LEN: usize = 16;

/// Normalises a user-entered extension: trims, removes one leading dot,
/// lower-cases, and accepts only ASCII letters, digits, `_` and `-`.
///
/// Fixes 2.x bug B10 (no validation, prefix-based duplicate check).
pub fn normalize_manual_extension(input: &str) -> Result<String> {
    let trimmed = input.trim();
    let without_dot = trimmed.strip_prefix('.').unwrap_or(trimmed);
    let valid = !without_dot.is_empty()
        && without_dot.len() <= MAX_MANUAL_EXTENSION_LEN
        && without_dot
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
    if valid {
        Ok(without_dot.to_ascii_lowercase())
    } else {
        Err(CoreError::InvalidExtension(input.to_string()))
    }
}

/// The user's format selection.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct FormatSelection {
    pub enabled: BTreeSet<FormatId>,
    /// Normalised manual extensions (see [`normalize_manual_extension`]).
    #[serde(default)]
    pub manual: BTreeSet<String>,
    #[serde(default)]
    pub include_all: bool,
}

/// Owned, serialisable view of one registry entry for the GUI.
#[derive(Debug, Clone, Serialize)]
#[cfg_attr(feature = "typescript", derive(ts_rs::TS))]
#[cfg_attr(feature = "typescript", ts(export))]
pub struct FormatDescriptor {
    pub id: FormatId,
    pub raw: bool,
    pub extensions: Vec<String>,
    pub vendor: Option<String>,
}

/// The registry as the GUI sees it; the frontend keeps no extension list.
pub fn descriptors() -> Vec<FormatDescriptor> {
    REGISTRY
        .iter()
        .map(|format| FormatDescriptor {
            id: format.id,
            raw: format.category == FormatCategory::Raw,
            extensions: format.extensions.iter().map(|extension| (*extension).to_string()).collect(),
            vendor: format.vendor.map(str::to_string),
        })
        .collect()
}

impl FormatSelection {
    /// The 2.x default: only ARW enabled.
    pub fn legacy_default() -> Self {
        FormatSelection {
            enabled: [FormatId::Arw].into_iter().collect(),
            ..Default::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.include_all && self.enabled.is_empty() && self.manual.is_empty()
    }

    /// Adds a manual extension after validation. Returns `false` if it was
    /// already present (exact match, case-insensitive).
    pub fn add_manual(&mut self, input: &str) -> Result<bool> {
        let ext = normalize_manual_extension(input)?;
        Ok(self.manual.insert(ext))
    }

    pub fn matcher(&self) -> FormatMatcher {
        let mut extensions = HashSet::new();
        for id in &self.enabled {
            for ext in info(*id).extensions {
                extensions.insert((*ext).to_string());
            }
        }
        extensions.extend(self.manual.iter().cloned());
        FormatMatcher {
            include_all: self.include_all,
            extensions,
        }
    }
}

/// Fast, case-insensitive extension filter built from a [`FormatSelection`].
#[derive(Debug, Clone)]
pub struct FormatMatcher {
    include_all: bool,
    extensions: HashSet<String>,
}

impl FormatMatcher {
    pub fn matches(&self, path: &Path) -> bool {
        if self.include_all {
            return true;
        }
        match path.extension().and_then(|e| e.to_str()) {
            Some(ext) => self.extensions.contains(&ext.to_ascii_lowercase()),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(ids: &[FormatId]) -> FormatSelection {
        FormatSelection {
            enabled: ids.iter().copied().collect(),
            ..Default::default()
        }
    }

    #[test]
    fn matching_is_case_insensitive() {
        let m = sel(&[FormatId::Arw]).matcher();
        assert!(m.matches(Path::new("DSC0001.ARW")));
        assert!(m.matches(Path::new("dsc0001.arw")));
        assert!(m.matches(Path::new("x/y/DSC.ArW")));
        assert!(!m.matches(Path::new("DSC0001.NEF")));
        assert!(!m.matches(Path::new("noext")));
    }

    #[test]
    fn tiff_and_heic_cover_their_aliases() {
        let m = sel(&[FormatId::Tiff, FormatId::Heic]).matcher();
        for f in ["a.tif", "a.TIFF", "a.heic", "a.HEIF"] {
            assert!(m.matches(Path::new(f)), "{f}");
        }
    }

    #[test]
    fn jpg_and_jpeg_are_separate_toggles() {
        let m = sel(&[FormatId::Jpg]).matcher();
        assert!(m.matches(Path::new("a.jpg")));
        assert!(!m.matches(Path::new("a.jpeg")));
    }

    #[test]
    fn include_all_matches_everything() {
        let s = FormatSelection {
            include_all: true,
            ..Default::default()
        };
        assert!(s.matcher().matches(Path::new("anything.xyz")));
        assert!(s.matcher().matches(Path::new("no_extension")));
    }

    #[test]
    fn manual_extensions_are_validated_and_normalised() {
        assert_eq!(normalize_manual_extension(".XMP").unwrap(), "xmp");
        assert_eq!(normalize_manual_extension("  mp4 ").unwrap(), "mp4");
        for bad in [
            "",
            ".",
            "a b",
            "../x",
            "a/b",
            "é",
            "x".repeat(17).as_str(),
            "..x",
        ] {
            assert!(normalize_manual_extension(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn manual_duplicate_check_is_exact_not_prefix() {
        // 2.x bug B10: ".cr" was rejected when ".cr3" existed.
        let mut s = FormatSelection::default();
        assert!(s.add_manual("cr3").unwrap());
        assert!(s.add_manual(".cr").unwrap());
        assert!(!s.add_manual(".CR3").unwrap());
        let m = s.matcher();
        assert!(m.matches(Path::new("a.cr")));
        assert!(m.matches(Path::new("a.cr3")));
    }

    #[test]
    fn registry_contains_every_id_once() {
        let ids: HashSet<_> = REGISTRY.iter().map(|f| f.id).collect();
        assert_eq!(ids.len(), REGISTRY.len());
        assert_eq!(REGISTRY.len(), 13);
    }

    #[test]
    fn legacy_default_is_arw_only() {
        let s = FormatSelection::legacy_default();
        assert_eq!(s.enabled.len(), 1);
        assert!(s.enabled.contains(&FormatId::Arw));
    }

    #[test]
    fn descriptors_mirror_registry() {
        let descriptors = descriptors();
        assert_eq!(descriptors.len(), REGISTRY.len());
        for (descriptor, format) in descriptors.iter().zip(REGISTRY) {
            assert_eq!(descriptor.id, format.id);
            assert_eq!(descriptor.raw, format.category == FormatCategory::Raw);
            assert_eq!(
                descriptor.extensions,
                format.extensions.iter().map(|extension| (*extension).to_string()).collect::<Vec<_>>()
            );
            assert_eq!(descriptor.vendor.as_deref(), format.vendor);
        }
    }
}

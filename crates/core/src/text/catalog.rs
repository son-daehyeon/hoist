use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use cosmic_text::fontdb::{self, Source};
use serde::{Deserialize, Serialize};
use ttf_parser::Tag;

use super::Fonts;

pub(super) struct CatalogEntry {
    display_name: String,
    category: String,
    weight: Option<u16>,
}

#[derive(Deserialize)]
struct ManifestEntry {
    display_name: String,
    category: String,
    #[serde(default)]
    weights: Vec<String>,
    #[serde(default)]
    files: Vec<String>,
}

pub const FONT_CATEGORIES: [&str; 5] = ["고딕", "명조", "둥근", "손글씨", "디스플레이"];

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum FontStyle {
    Normal,
    Italic,
    Oblique,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFace {
    pub family: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub bundled: bool,
    pub weights: Vec<u16>,
    pub style: FontStyle,
    pub path: PathBuf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FontFamily {
    pub family: String,
    pub display_name: Option<String>,
    pub category: Option<String>,
    pub bundled: bool,
    pub weights: Vec<u16>,
    pub faces: Vec<FontFace>,
}

impl Fonts {
    pub fn catalog(&self) -> Vec<FontFamily> {
        let mut by_family: BTreeMap<String, FontFamily> = BTreeMap::new();
        for face in self.faces() {
            let f = by_family
                .entry(face.family.clone())
                .or_insert_with(|| FontFamily {
                    family: face.family.clone(),
                    display_name: None,
                    category: None,
                    bundled: false,
                    weights: Vec::new(),
                    faces: Vec::new(),
                });
            f.bundled |= face.bundled;
            f.display_name = f.display_name.take().or_else(|| face.display_name.clone());
            f.category = f.category.take().or_else(|| face.category.clone());
            f.weights.extend(&face.weights);
            f.faces.push(face);
        }
        let rank = |c: &Option<String>| {
            c.as_deref()
                .and_then(|c| FONT_CATEGORIES.iter().position(|k| *k == c))
                .unwrap_or(FONT_CATEGORIES.len())
        };
        let mut out: Vec<FontFamily> = by_family
            .into_values()
            .map(|mut f| {
                f.weights.sort_unstable();
                f.weights.dedup();
                f.faces.sort_by_key(|x| x.weights.first().copied());
                f
            })
            .collect();
        out.sort_by(|a, b| {
            rank(&a.category).cmp(&rank(&b.category)).then_with(|| {
                a.display_name
                    .as_ref()
                    .unwrap_or(&a.family)
                    .cmp(b.display_name.as_ref().unwrap_or(&b.family))
            })
        });
        out
    }

    pub fn families(&self) -> Vec<String> {
        let font_system = self.lock();
        let mut names: Vec<String> = font_system
            .db()
            .faces()
            .flat_map(|f| f.families.iter().map(|(n, _)| n.clone()))
            .collect();
        names.sort();
        names.dedup();
        names
    }

    /// Variable fonts list every hundred weight their `wght` axis covers.
    pub fn faces(&self) -> Vec<FontFace> {
        let font_system = self.lock();
        let db = font_system.db();
        db.faces()
            .filter_map(|f| {
                let path = dunce::simplified(source_path(&f.source)?).to_path_buf();
                let entry = file_name(&path).and_then(|n| self.manifest.get(n));
                let style = match f.style {
                    fontdb::Style::Normal => FontStyle::Normal,
                    fontdb::Style::Italic => FontStyle::Italic,
                    fontdb::Style::Oblique => FontStyle::Oblique,
                };
                Some(FontFace {
                    family: f.families.first()?.0.clone(),
                    display_name: entry.map(|e| e.display_name.clone()),
                    category: entry.map(|e| e.category.clone()),
                    bundled: self.bundled_dir.as_ref().is_some_and(|d| path.starts_with(d)),
                    weights: db
                        .with_face_data(f.id, variable_weights)
                        .flatten()
                        .unwrap_or_else(|| vec![f.weight.0]),
                    style,
                    path,
                })
            })
            .collect()
    }
}

fn source_path(source: &Source) -> Option<&Path> {
    match source {
        Source::File(p) | Source::SharedFile(p, _) => Some(p),
        Source::Binary(_) => None,
    }
}

fn file_name(path: &Path) -> Option<&str> {
    path.file_name().and_then(|n| n.to_str())
}

fn weight_named(name: &str) -> Option<u16> {
    let key: String = name
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    Some(match key.as_str() {
        "thin" | "hairline" => 100,
        "extralight" | "ultralight" => 200,
        "light" => 300,
        "regular" | "normal" | "book" => 400,
        "medium" => 500,
        "semibold" | "demibold" => 600,
        "bold" => 700,
        "extrabold" | "ultrabold" => 800,
        "black" | "heavy" => 900,
        _ => return None,
    })
}

pub(super) fn read_manifest(dir: &Path) -> HashMap<String, CatalogEntry> {
    let entries: Vec<ManifestEntry> = fs::read_to_string(dir.join("manifest.json"))
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default();
    let mut out = HashMap::new();
    for e in entries {
        let paired = e.weights.len() == e.files.len();
        for (i, file) in e.files.iter().enumerate() {
            let weight = paired
                .then(|| e.weights.get(i))
                .flatten()
                .and_then(|w| weight_named(w));
            out.insert(
                file.clone(),
                CatalogEntry {
                    display_name: e.display_name.clone(),
                    category: e.category.clone(),
                    weight,
                },
            );
        }
    }
    out
}

/// Some bundled fonts report usWeightClass 400 for every style (Suncheon R/B, MuseumClassic L/M/B), so the manifest's weight wins.
pub(super) fn fix_weights(db: &mut fontdb::Database, manifest: &HashMap<String, CatalogEntry>) {
    let fixes: Vec<(fontdb::ID, u16)> = db
        .faces()
        .filter_map(|f| {
            let w = manifest.get(file_name(source_path(&f.source)?)?)?.weight?;
            (w != f.weight.0).then_some((f.id, w))
        })
        .collect();
    for (id, w) in fixes {
        if let Some(mut info) = db.face(id).cloned() {
            db.remove_face(id);
            info.weight = fontdb::Weight(w);
            db.push_face_info(info);
        }
    }
}

pub(super) fn variable_weights(data: &[u8], index: u32) -> Option<Vec<u16>> {
    let raw = ttf_parser::RawFace::parse(data, index).ok()?;
    let fvar = ttf_parser::fvar::Table::parse(raw.table(Tag::from_bytes(b"fvar"))?)?;
    let axis = fvar
        .axes
        .into_iter()
        .find(|a| a.tag == Tag::from_bytes(b"wght"))?;
    let weights: Vec<u16> = (1..=9u16)
        .map(|i| i * 100)
        .filter(|w| (axis.min_value..=axis.max_value).contains(&f32::from(*w)))
        .collect();
    (!weights.is_empty()).then_some(weights)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::bundled_fonts_dir;
    use crate::model::TextProps;

    #[test]
    fn manifest_names_sorts_and_fixes_weights() {
        let bundled = bundled_fonts_dir();
        if !bundled.join("manifest.json").is_file() {
            return;
        }
        let fonts = Fonts::new(Some(&bundled));
        let faces = fonts.faces();
        let weights_of = |prefix: &str| {
            let mut w: Vec<u16> = faces
                .iter()
                .filter(|f| file_name(&f.path).is_some_and(|n| n.starts_with(prefix)))
                .flat_map(|f| f.weights.clone())
                .collect();
            w.sort_unstable();
            w.dedup();
            w
        };
        assert_eq!(weights_of("Suncheon-"), [400, 700]);
        assert_eq!(weights_of("MuseumClassic-"), [300, 500, 700]);

        let catalog = fonts.catalog();
        let pretendard = catalog
            .iter()
            .find(|f| f.family == "Pretendard")
            .expect("Pretendard");
        assert_eq!(pretendard.category.as_deref(), Some("고딕"));
        assert!(pretendard.display_name.is_some());
        let ranks: Vec<usize> = catalog
            .iter()
            .map(|f| {
                f.category
                    .as_deref()
                    .and_then(|c| FONT_CATEGORIES.iter().position(|k| *k == c))
                    .unwrap_or(FONT_CATEGORIES.len())
            })
            .collect();
        assert!(
            ranks.windows(2).all(|w| w[0] <= w[1]),
            "sorted by category {ranks:?}"
        );

        let family = faces
            .iter()
            .find(|f| file_name(&f.path) == Some("Suncheon-Bold.ttf"))
            .expect("Suncheon bold")
            .family
            .clone();
        let mut p = TextProps {
            text: "순천".into(),
            font_family: family,
            font_weight: 400,
            font_size_mm: 20.0,
            ..Default::default()
        };
        let regular = fonts.layout_to_path(&p, 100.0, 30.0).expect("regular").path_d;
        p.font_weight = 700;
        let bold = fonts.layout_to_path(&p, 100.0, 30.0).expect("bold").path_d;
        assert_ne!(regular, bold, "weight 700 picks the Bold file");
    }

    #[test]
    fn faces_list_files_and_expand_variable_weights() {
        let bundled = std::fs::canonicalize(bundled_fonts_dir()).expect("bundled fonts dir");
        let faces = Fonts::new(Some(&bundled)).faces();
        assert!(
            faces
                .iter()
                .all(|f| !f.path.to_string_lossy().starts_with(r"\\?\")),
            "no verbatim prefix"
        );
        assert!(faces.iter().any(|f| f.bundled) && faces.iter().any(|f| !f.bundled));
        let pretendard: Vec<&FontFace> = faces.iter().filter(|f| f.family == "Pretendard").collect();
        let mut weights: Vec<u16> = pretendard.iter().flat_map(|f| f.weights.clone()).collect();
        weights.sort_unstable();
        weights.dedup();
        assert!(
            [400, 500, 600, 700, 800, 900].iter().all(|w| weights.contains(w)),
            "{weights:?}"
        );
        assert!(pretendard.iter().any(|f| f.bundled));
        assert!(pretendard.iter().all(|f| f.path.is_file()));
        if let Some(bahnschrift) = faces.iter().find(|f| f.family == "Bahnschrift") {
            assert!(bahnschrift.weights.len() > 1, "variable axis {bahnschrift:?}");
        }
    }
}

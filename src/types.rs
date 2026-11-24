use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub protected_package: bool,
    pub installed_kib: Option<u64>,
    pub group: String,
}

#[derive(Debug, Clone)]
pub struct Image {
    pub path: PathBuf,
    pub release: String,
    pub distro_name: String,
    pub supported_locales: Vec<String>,
    pub packages: Vec<Package>,
    pub has_flatpak: bool,
    pub flatpak_bytes: u64,
    pub squash_offset: u64,
    pub squash_iso_path: String,
}

#[derive(Debug, Clone)]
pub struct Plan {
    pub image: Image,
    pub original_release: String,
    pub remove: BTreeSet<String>,
    pub replaced_folders: BTreeMap<String, PathBuf>,
    pub debs: Vec<PathBuf>,
    pub locales: Vec<String>,
    pub default_locale: Option<String>,
    pub language_debs: Vec<PathBuf>,
    pub clean_flatpak: bool,
}

impl Plan {
    pub fn new(image: Image) -> Self {
        let original_release = image.release.clone();
        Self {
            image,
            original_release,
            remove: BTreeSet::new(),
            replaced_folders: BTreeMap::new(),
            debs: Vec::new(),
            locales: Vec::new(),
            default_locale: None,
            language_debs: Vec::new(),
            clean_flatpak: false,
        }
    }
}

pub struct IsoReplacement {
    pub iso_path: String,
    pub local: PathBuf,
}

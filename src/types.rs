use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DistroType {
    DebianUbuntu = 0,
    ArchLinux = 1,
}

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
    pub distro: DistroType,
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
    pub desktop_installer: bool,
    pub desktop_refresh: bool,
    pub original_compression: bool,
    pub desktop_groups: BTreeSet<String>,
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
            desktop_installer: false,
            desktop_refresh: false,
            original_compression: false,
            desktop_groups: BTreeSet::new(),
        }
    }
}

pub struct IsoReplacement {
    pub iso_path: String,
    pub local: PathBuf,
}

pub fn package_name(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    let base = if let Some(stripped) = name.strip_suffix(":amd64") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(":i386") {
        stripped
    } else if let Some(stripped) = name.strip_suffix(":all") {
        stripped
    } else {
        name
    };

    if base.len() < 2 {
        return false;
    }
    let first = base.as_bytes()[0];
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return false;
    }
    base.bytes().skip(1).all(|b| {
        b.is_ascii_lowercase()
            || b.is_ascii_digit()
            || b == b'+'
            || b == b'.'
            || b == b'-'
    })
}

pub fn protected_package(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "apt", "dpkg", "systemd", "linux", "grub", "shim", "casper", "ubiquity", "zorin",
        "gnome-shell", "gnome-session", "gdm3", "lightdm", "xorg", "xserver", "wayland",
        "network-manager", "netplan", "initramfs", "init", "libc6", "libgcc", "libstdc++6",
        "bash", "dash", "coreutils", "sudo", "dbus", "passwd", "base",
        "ubuntu-desktop", "ubuntu-minimal", "ubuntu-standard", "ubuntu-drivers",
        "debian-installer", "live-boot", "live-config", "calamares", "mintsystem",
        "pipewire", "wireplumber",
    ];
    for &prefix in PREFIXES {
        if name.starts_with(prefix) {
            if name.len() == prefix.len()
                || name.as_bytes()[prefix.len()] == b'-'
                || name.as_bytes()[prefix.len()] == b':'
            {
                return true;
            }
        }
    }
    false
}

pub fn app_group(name: &str) -> String {
    const FAMILIES: &[&str] = &[
        "7zip", "baobab", "brasero", "deja-dup", "eog", "evince", "file-roller",
        "gimp", "gnome-calculator", "gnome-calendar", "gnome-characters",
        "gnome-clocks", "gnome-contacts", "gnome-logs", "gnome-maps", "gnome-weather",
        "gparted", "libreoffice", "remmina", "rhythmbox", "shotwell", "simple-scan", "totem",
    ];
    for &family in FAMILIES {
        if name == family
            || (name.starts_with(family) && name.as_bytes().get(family.len()) == Some(&b'-'))
        {
            return family.to_string();
        }
    }
    if name == "p7zip-full" {
        return "7zip".to_string();
    }
    if name.starts_with("brave") {
        return "brave".to_string();
    }
    if name == "libbrasero-media3-1" {
        return "brasero".to_string();
    }
    if name == "python3-uno" {
        return "libreoffice".to_string();
    }
    if name == "libevolution"
        || name.starts_with("evolution-ews")
        || name.starts_with("evolution-plugin")
        || name == "evolution"
        || name == "evolution-common"
    {
        return "evolution".to_string();
    }
    if name.starts_with("gir1.2-gimp-")
        || name.starts_with("libgimp-")
        || name == "libgimp2.0t64"
        || name == "libgmic1"
    {
        return "gimp".to_string();
    }
    if name.starts_with("gir1.2-rb-") || name.starts_with("librhythmbox-core") {
        return "rhythmbox".to_string();
    }
    String::new()
}

pub fn arch_app_group(name: &str) -> String {
    if name == "firefox" || name.starts_with("firefox-") {
        return "firefox".to_string();
    }
    if name == "alacritty" || name == "cachyos-alacritty-config" {
        return "alacritty".to_string();
    }
    if name == "kate" {
        return "kate".to_string();
    }
    if name == "kcalc" {
        return "kcalc".to_string();
    }
    if name == "spectacle" {
        return "spectacle".to_string();
    }
    if name == "dolphin" {
        return "dolphin".to_string();
    }
    if name == "konsole" {
        return "konsole".to_string();
    }
    if name == "partitionmanager" {
        return "partitionmanager".to_string();
    }
    if name == "cachyos-hello" {
        return "cachyos-hello".to_string();
    }
    if name == "fish" || name == "fisher" || name.starts_with("fish-") {
        return "fish".to_string();
    }
    if name == "nano" || name.starts_with("nano-") {
        return "nano".to_string();
    }
    if name == "vim" || name.starts_with("vim-") {
        return "vim".to_string();
    }
    if name.starts_with("libreoffice") {
        return "libreoffice".to_string();
    }
    if name.starts_with("brave") {
        return "brave".to_string();
    }
    if name == "thunderbird" {
        return "thunderbird".to_string();
    }
    if name == "vlc" {
        return "vlc".to_string();
    }
    if name == "mpv" {
        return "mpv".to_string();
    }
    if name == "gimp" {
        return "gimp".to_string();
    }
    String::new()
}

pub fn arch_protected_package(name: &str) -> bool {
    const PREFIXES: &[&str] = &[
        "linux", "systemd", "pacman", "glibc", "coreutils", "bash", "archlinux-", "cachyos-keyring",
        "cachyos-mirrorlist", "cachyos-settings", "cachyos-hooks", "calamares", "xorg", "wayland",
        "kwin", "plasma-", "pipewire", "wireplumber", "sudo", "dbus", "polkit", "filesystem",
        "shadow", "util-linux", "pam", "mkinitcpio", "dracut", "grub", "efibootmgr", "btrfs-progs",
        "xfsprogs", "e2fsprogs", "dosfstools", "zfs", "cryptsetup", "device-mapper", "lvm2",
        "networkmanager", "iwd", "wpa_supplicant", "mesa", "vulkan", "gcc-libs",
    ];
    for &prefix in PREFIXES {
        if name.starts_with(prefix) {
            return true;
        }
    }
    false
}

pub fn select_group(plan: &mut Plan, group: &str, selected: bool) {
    let mut found = false;
    for package in &plan.image.packages {
        if package.protected_package || package.group != group {
            continue;
        }
        found = true;
        if selected {
            plan.remove.insert(package.name.clone());
        } else {
            plan.remove.remove(&package.name);
        }
    }
    if !found {
        eprintln!("Unknown removable app group: {}", group);
    }
    if selected {
        plan.desktop_groups.remove(group);
    }
}

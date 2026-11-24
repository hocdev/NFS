pub mod reader;
pub mod writer;

pub use reader::{read_squash_file, detect_flatpak, visit_pacman_descriptions};
pub use writer::rebuild_squashfs;

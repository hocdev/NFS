/// Populated from package metadata by build-support; no fixed package or locale.
pub struct LocalePack {
    pub abi: &'static str,
    pub architecture: &'static str,
    pub catalog: &'static str,
    pub data: &'static [u8],
}
include!(concat!(env!("OUT_DIR"), "/locale-packs.rs"));

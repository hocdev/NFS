use std::path::PathBuf;
pub struct FileDialog {
    title: String,
    name: String,
    filters: Vec<(String, Vec<String>)>,
    owner: *mut std::ffi::c_void,
}
impl FileDialog {
    pub fn new() -> Self {
        Self {
            title: String::new(),
            name: String::new(),
            filters: Vec::new(),
            owner: std::ptr::null_mut(),
        }
    }
    pub fn set_owner(mut self, owner: *mut std::ffi::c_void) -> Self {
        self.owner = owner;
        self
    }
    pub fn add_filter(mut self, name: &str, extensions: &[&str]) -> Self {
        self.filters.push((
            name.into(),
            extensions.iter().map(|s| s.to_string()).collect(),
        ));
        self
    }
    pub fn set_file_name(mut self, name: impl AsRef<str>) -> Self {
        self.name = name.as_ref().into();
        self
    }
    pub fn set_title(mut self, title: impl AsRef<str>) -> Self {
        self.title = title.as_ref().into();
        self
    }
    pub fn pick_file(self) -> Option<PathBuf> {
        self.files(false, false)?.into_iter().next()
    }
    pub fn pick_files(self) -> Option<Vec<PathBuf>> {
        self.files(false, true)
    }
    pub fn save_file(self) -> Option<PathBuf> {
        self.files(true, false)?.into_iter().next()
    }
    #[cfg(windows)]
    fn files(self, save: bool, multi: bool) -> Option<Vec<PathBuf>> {
        use std::ffi::OsString;
        use std::os::windows::ffi::{OsStrExt, OsStringExt};
        let mut buffer = vec![0u16; 65536];
        let initial: Vec<u16> = std::ffi::OsStr::new(&self.name).encode_wide().collect();
        if initial.len() >= buffer.len() {
            return None;
        }
        buffer[..initial.len()].copy_from_slice(&initial);
        let default_ext: Option<Vec<u16>> = if save {
            self.filters
                .first()
                .and_then(|(_, exts)| exts.first())
                .map(|ext| ext.encode_utf16().chain([0]).collect())
        } else {
            None
        };
        let mut filter = Vec::new();
        for (label, extensions) in self.filters {
            filter.extend(label.encode_utf16());
            filter.push(0);
            filter.extend(
                extensions
                    .iter()
                    .map(|s| format!("*.{s}"))
                    .collect::<Vec<_>>()
                    .join(";")
                    .encode_utf16(),
            );
            filter.push(0);
        }
        filter.push(0);
        let title: Vec<u16> = self.title.encode_utf16().chain([0]).collect();
        let mut ofn: OpenFileName = unsafe { std::mem::zeroed() };
        ofn.size = std::mem::size_of::<OpenFileName>() as u32;
        ofn.owner = self.owner;
        ofn.filter = filter.as_ptr();
        ofn.file = buffer.as_mut_ptr();
        ofn.max_file = buffer.len() as u32;
        ofn.title = title.as_ptr();
        if let Some(ref ext) = default_ext {
            ofn.default_ext = ext.as_ptr();
        }
        ofn.flags =
            0x80000 | 0x800 | 8 | if save { 2 } else { 0x1000 } | if multi { 0x200 } else { 0 };
        let ok = unsafe {
            if save {
                GetSaveFileNameW(&mut ofn)
            } else {
                GetOpenFileNameW(&mut ofn)
            }
        };
        if ok == 0 {
            return None;
        }
        let parts: Vec<PathBuf> = buffer
            .split(|&c| c == 0)
            .take_while(|s| !s.is_empty())
            .map(|s| PathBuf::from(OsString::from_wide(s)))
            .collect();
        if parts.len() < 2 {
            Some(parts)
        } else {
            Some(parts[1..].iter().map(|p| parts[0].join(p)).collect())
        }
    }
    #[cfg(not(windows))]
    fn files(self, _save: bool, _multi: bool) -> Option<Vec<PathBuf>> {
        None
    }
    #[cfg(windows)]
    pub fn pick_folder(self) -> Option<PathBuf> {
        use std::os::windows::ffi::OsStringExt;
        let title: Vec<u16> = self.title.encode_utf16().chain([0]).collect();
        let mut display = [0u16; 260];
        let info = BrowseInfo {
            owner: self.owner,
            root: std::ptr::null(),
            display: display.as_mut_ptr(),
            title: title.as_ptr(),
            flags: 0x41,
            callback: std::ptr::null(),
            param: 0,
            image: 0,
        };
        unsafe {
            CoInitializeEx(std::ptr::null_mut(), 2);
            let list = SHBrowseForFolderW(&info);
            if list.is_null() {
                return None;
            }
            let mut path = [0u16; 260];
            let ok = SHGetPathFromIDListW(list, path.as_mut_ptr());
            CoTaskMemFree(list);
            if ok == 0 {
                return None;
            }
            let n = path.iter().position(|&c| c == 0)?;
            Some(PathBuf::from(std::ffi::OsString::from_wide(&path[..n])))
        }
    }
    #[cfg(not(windows))]
    pub fn pick_folder(self) -> Option<PathBuf> {
        None
    }
}
#[cfg(windows)]
#[repr(C)]
struct OpenFileName {
    size: u32,
    owner: *mut std::ffi::c_void,
    instance: *mut std::ffi::c_void,
    filter: *const u16,
    custom: *mut u16,
    max_custom: u32,
    index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_title: u32,
    initial: *const u16,
    title: *const u16,
    flags: u32,
    offset: u16,
    extension: u16,
    default_ext: *const u16,
    data: isize,
    hook: *const std::ffi::c_void,
    template: *const u16,
    reserved: *mut std::ffi::c_void,
    reserved2: u32,
    flags_ex: u32,
}
#[cfg(windows)]
#[repr(C)]
struct BrowseInfo {
    owner: *mut std::ffi::c_void,
    root: *const std::ffi::c_void,
    display: *mut u16,
    title: *const u16,
    flags: u32,
    callback: *const std::ffi::c_void,
    param: isize,
    image: i32,
}
#[cfg(windows)]
#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(info: *mut OpenFileName) -> i32;
    fn GetSaveFileNameW(info: *mut OpenFileName) -> i32;
}
#[cfg(windows)]
#[link(name = "shell32")]
extern "system" {
    fn SHBrowseForFolderW(info: *const BrowseInfo) -> *mut std::ffi::c_void;
    fn SHGetPathFromIDListW(list: *const std::ffi::c_void, path: *mut u16) -> i32;
}
#[cfg(windows)]
#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut std::ffi::c_void, flags: u32) -> i32;
    fn CoTaskMemFree(value: *mut std::ffi::c_void);
}
#[cfg(windows)]
#[link(name = "user32")]
extern "system" {
    fn MessageBoxW(
        hwnd: *mut std::ffi::c_void,
        text: *const u16,
        caption: *const u16,
        kind: u32,
    ) -> i32;
}

#[cfg(windows)]
pub fn alert_error(title: &str, message: &str) {
    let t: Vec<u16> = title.encode_utf16().chain([0]).collect();
    let m: Vec<u16> = message.encode_utf16().chain([0]).collect();
    unsafe {
        MessageBoxW(std::ptr::null_mut(), m.as_ptr(), t.as_ptr(), 0x0000_0010);
    }
}

#[cfg(not(windows))]
pub fn alert_error(_title: &str, message: &str) {
    eprintln!("{message}");
}

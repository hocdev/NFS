#![windows_subsystem = "windows"]
#![allow(dead_code)]
#![allow(unused_imports)]

use slint::{ComponentHandle, Model, ModelRc, VecModel};
use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

mod checksum;
mod dialog;
mod engine;
mod iso;
mod language;
mod squash;
mod types;
mod xz;

use engine::{build_iso, export_package_files, extract_iso, inspect_iso};
use types::{Image, Package, Plan};

#[cfg(target_os = "windows")]
mod win32_icon {
    type HWND = *mut std::ffi::c_void;
    type BOOL = i32;
    type LPARAM = isize;
    type WPARAM = usize;
    type LRESULT = isize;
    type HICON = *mut std::ffi::c_void;
    type HMODULE = *mut std::ffi::c_void;

    extern "system" {
        fn GetModuleHandleW(lpModuleName: *const u16) -> HMODULE;
        fn LoadImageW(
            hInst: HMODULE,
            name: *const u16,
            type_: u32,
            cx: i32,
            cy: i32,
            fuLoad: u32,
        ) -> *mut std::ffi::c_void;
        fn LoadIconW(hInstance: HMODULE, lpIconName: *const u16) -> HICON;
        fn EnumWindows(
            lpEnumFunc: unsafe extern "system" fn(HWND, LPARAM) -> BOOL,
            lParam: LPARAM,
        ) -> BOOL;
        fn GetWindowTextW(hWnd: HWND, lpString: *mut u16, nMaxCount: i32) -> i32;
        fn SendMessageW(hWnd: HWND, Msg: u32, wParam: WPARAM, lParam: LPARAM) -> LRESULT;
        fn GetWindowLongW(hWnd: HWND, nIndex: i32) -> i32;
        fn SetWindowLongW(hWnd: HWND, nIndex: i32, dwNewLong: i32) -> i32;
    }

    const WM_SETICON: u32 = 0x0080;
    const ICON_SMALL: WPARAM = 0;
    const ICON_BIG: WPARAM = 1;
    const IMAGE_ICON: u32 = 1;
    const LR_DEFAULTSIZE: u32 = 0x0040;
    const LR_SHARED: u32 = 0x8000;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_APPWINDOW: i32 = 0x00040000;

    unsafe extern "system" fn enum_windows_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let mut buffer = [0u16; 512];
        let len = GetWindowTextW(hwnd, buffer.as_mut_ptr(), 512);
        if len > 0 {
            let title = String::from_utf16_lossy(&buffer[..len as usize]);
            if title.contains("ISOLite") {
                let h_icon = lparam as HICON;
                SendMessageW(hwnd, WM_SETICON, ICON_BIG, h_icon as LPARAM);
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL, h_icon as LPARAM);
                let ex = GetWindowLongW(hwnd, GWL_EXSTYLE);
                SetWindowLongW(hwnd, GWL_EXSTYLE, ex | WS_EX_APPWINDOW);
                return 1;
            }
        }
        1
    }

    pub fn apply_taskbar_icon() {
        unsafe {
            let hinst = GetModuleHandleW(std::ptr::null());
            let mut icon = LoadImageW(
                hinst,
                1 as *const u16,
                IMAGE_ICON,
                0,
                0,
                LR_DEFAULTSIZE | LR_SHARED,
            ) as HICON;
            if icon.is_null() {
                icon = LoadIconW(hinst, 1 as *const u16);
            }
            if !icon.is_null() {
                EnumWindows(enum_windows_callback, icon as LPARAM);
            }
        }
    }
}

slint::include_modules!();

fn format_bytes(bytes: u64) -> String {
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{} KB", bytes / 1024)
    } else {
        format!("{} B", bytes)
    }
}

fn format_kib(kib: Option<u64>) -> String {
    match kib {
        Some(k) => {
            if k >= 1024 {
                format!("{:.1} MB", k as f64 / 1024.0)
            } else {
                format!("{} KB", k)
            }
        }
        None => "-".to_string(),
    }
}

fn apply_package_filter(
    ui: &MainWindow,
    all_packages: &[Package],
    selected_removals: &BTreeSet<String>,
    replaced_folders: &BTreeMap<String, PathBuf>,
    tab: i32,
    query: &str,
    sort_col: i32,
    sort_asc: bool,
) {
    let q_lower = query.trim().to_lowercase();
    let mut matching: Vec<&Package> = Vec::new();

    for p in all_packages {
        // Tab 0: Tất cả, Tab 1: Ứng dụng bên thứ 3, Tab 2: Gói hệ thống
        let tab_match = match tab {
            1 => !p.protected_package && !p.group.is_empty(),
            2 => p.protected_package || p.group.is_empty(),
            _ => true,
        };

        if !tab_match {
            continue;
        }

        if !q_lower.is_empty() {
            let name_match = p.name.to_lowercase().contains(&q_lower);
            let grp_match = p.group.to_lowercase().contains(&q_lower);
            if !name_match && !grp_match {
                continue;
            }
        }

        matching.push(p);
    }

    // Sort matching packages
    matching.sort_by(|a, b| {
        match sort_col {
            1 => {
                // Size sort
                let sz_a = a.installed_kib.unwrap_or(0);
                let sz_b = b.installed_kib.unwrap_or(0);
                let cmp = sz_a.cmp(&sz_b);
                let res = if sort_asc { cmp } else { cmp.reverse() };
                res.then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            }
            _ => {
                // Name sort
                let cmp = a.name.to_lowercase().cmp(&b.name.to_lowercase());
                if sort_asc {
                    cmp
                } else {
                    cmp.reverse()
                }
            }
        }
    });

    let mut items = Vec::with_capacity(matching.len());
    for p in matching {
        let is_selected = selected_removals.contains(&p.name);
        let rep_opt = replaced_folders.get(&p.name);
        let is_rep = rep_opt.is_some();
        let rep_path = rep_opt
            .map(|pb| pb.display().to_string())
            .unwrap_or_default();

        items.push(PackageItem {
            name: p.name.clone().into(),
            version: p.version.clone().into(),
            group: p.group.clone().into(),
            is_protected: p.protected_package,
            size_str: format_kib(p.installed_kib).into(),
            selected: is_selected,
            is_replaced: is_rep,
            replaced_path: rep_path.into(),
        });
    }

    ui.set_packages(ModelRc::from(Rc::new(VecModel::from(items))));
    ui.set_selected_count(selected_removals.len() as i32);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    if args.get(1).is_some_and(|s| s == "--help" || s == "-h") {
        println!("ISOLite\ninspect <input.iso>\nextract <input.iso> <folder>\nbuild <input.iso> <output.iso> [--locale vi_VN.UTF-8] [--default-locale vi_VN.UTF-8] [--remove package] [--deb file.deb] [--language-deb file.deb]");
        return Ok(());
    }
    // CLI Mode
    if args.len() >= 3 {
        let cmd = &args[1];
        if cmd == "inspect" && args.len() == 3 {
            let p = PathBuf::from(&args[2]);
            let img = inspect_iso(&p, |msg| eprintln!("{}", msg))?;
            println!("Release: {}", img.release);
            println!(
                "Root filesystem: {} (offset {})",
                img.squash_iso_path, img.squash_offset
            );
            for path in [
                "usr/lib/os-release",
                "etc/debian_version",
                "etc/apt/sources.list",
            ] {
                if let Some(bytes) = squash::reader::read_squash_file(&p, img.squash_offset, path) {
                    println!("--- {} ---\n{}", path, String::from_utf8_lossy(&bytes));
                }
            }
            for p in &img.packages {
                println!(
                    "{}\t{}\t{}\t{}\t{}",
                    p.name,
                    p.version,
                    if p.protected_package {
                        "protected"
                    } else {
                        "selectable"
                    },
                    p.installed_kib
                        .map(|k| k.to_string())
                        .unwrap_or_else(|| "-".to_string()),
                    p.group
                );
            }
            return Ok(());
        }

        if cmd == "extract" && args.len() == 4 {
            let iso = PathBuf::from(&args[2]);
            let out = PathBuf::from(&args[3]);
            extract_iso(&iso, &out, |msg| eprintln!("{}", msg))?;
            return Ok(());
        }

        if cmd == "build" && args.len() >= 4 {
            let iso = PathBuf::from(&args[2]);
            let out = PathBuf::from(&args[3]);
            let img = inspect_iso(&iso, |msg| eprintln!("{}", msg))?;
            let orig = img.release.clone();
            let mut plan = Plan {
                image: img,
                original_release: orig,
                remove: BTreeSet::new(),
                debs: Vec::new(),
                locales: Vec::new(),
                default_locale: None,
                language_debs: Vec::new(),
                clean_flatpak: false,
                replaced_folders: BTreeMap::new(),
            };

            let mut i = 4;
            while i < args.len() {
                match args[i].as_str() {
                    "--locale" | "--default-locale" | "--language-deb" => {
                        let value = args.get(i + 1).ok_or("Thiếu giá trị tùy chọn ngôn ngữ")?;
                        match args[i].as_str() {
                            "--locale" => plan.locales.push(value.clone()),
                            "--default-locale" => plan.default_locale = Some(value.clone()),
                            _ => plan.language_debs.push(PathBuf::from(value)),
                        }
                        i += 1;
                    }
                    "--clean-flatpak" => plan.clean_flatpak = true,
                    "--remove" => {
                        if i + 1 < args.len() {
                            plan.remove.insert(args[i + 1].clone());
                            i += 1;
                        }
                    }
                    "--deb" => {
                        if i + 1 < args.len() {
                            plan.debs.push(PathBuf::from(&args[i + 1]));
                            i += 1;
                        }
                    }
                    _ => return Err(format!("Tùy chọn không hỗ trợ: {}", args[i]).into()),
                }
                i += 1;
            }

            build_iso(
                &plan,
                &out,
                |msg| eprintln!("{}", msg),
                |pct, msg| eprintln!("[{}%] {}", pct, msg),
            )?;
            return Ok(());
        }
    }

    // GUI Mode (Slint 1.18)
    let main_window = MainWindow::new()?;
    let ui_handle = main_window.as_weak();

    let current_image = Arc::new(Mutex::new(Option::<Image>::None));
    let staged_debs = Arc::new(Mutex::new(Vec::<PathBuf>::new()));
    let all_packages = Arc::new(Mutex::new(Vec::<Package>::new()));
    let selected_removals = Arc::new(Mutex::new(BTreeSet::<String>::new()));
    let replaced_folders = Arc::new(Mutex::new(BTreeMap::<String, PathBuf>::new()));
    let sort_column = Arc::new(Mutex::new(0i32));
    let sort_ascending = Arc::new(Mutex::new(true));
    // 1. Open ISO
    {
        let ui_weak = ui_handle.clone();
        let current_image = current_image.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_open_iso_clicked(move || {
            let file_opt = dialog::FileDialog::new()
                .add_filter("Ảnh đĩa ISO", &["iso"])
                .pick_file();

            if let Some(file_path) = file_opt {
                let ui_weak = ui_weak.clone();
                let current_image = current_image.clone();
                let all_packages = all_packages.clone();
                let selected_removals = selected_removals.clone();
                let replaced_folders = replaced_folders.clone();
                let sort_column = sort_column.clone();
                let sort_ascending = sort_ascending.clone();

                if let Some(ui) = ui_weak.upgrade() {
                    ui.set_status_bar_text("Đang đọc và phân tích cấu trúc tệp ISO...".into());
                    let mut log = ui.get_log_text().to_string();
                    log += &format!("\n▶ Đang mở tệp ISO: {}\n", file_path.display());
                    ui.set_log_text(log.into());
                }

                std::thread::spawn(move || {
                    let log_cb = {
                        let ui_weak = ui_weak.clone();
                        move |msg: &str| {
                            let msg_string = msg.to_string();
                            let ui_weak = ui_weak.clone();
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    let mut cur_log = ui.get_log_text().to_string();
                                    cur_log += &format!("{}\n", msg_string);
                                    ui.set_log_text(cur_log.into());
                                    ui.set_status_bar_text(msg_string.into());
                                }
                            });
                        }
                    };

                    match inspect_iso(&file_path, log_cb) {
                        Ok(img) => {
                            *all_packages.lock().unwrap() = img.packages.clone();
                            selected_removals.lock().unwrap().clear();
                            replaced_folders.lock().unwrap().clear();
                            *sort_column.lock().unwrap() = 0;
                            *sort_ascending.lock().unwrap() = true;

                            let all_packages_ui = all_packages.clone();
                            let selected_removals_ui = selected_removals.clone();
                            let replaced_folders_ui = replaced_folders.clone();

                            let distro_name = img.distro_name.clone();
                            let mut choices = vec!["Giữ nguyên".to_string()];
                            choices.extend(img.supported_locales.clone());

                            let has_flatpak = img.has_flatpak;
                            let flatpak_savings_str = format_bytes(img.flatpak_bytes);
                            let release_str = img.release.clone();
                            let path_str = img.path.display().to_string();
                            *current_image.lock().unwrap() = Some(img);

                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    ui.set_iso_loaded(true);
                                    ui.set_iso_path_text(path_str.into());
                                    ui.set_iso_release_text(release_str.clone().into());
                                    ui.set_iso_distro_badge(distro_name.into());
                                    ui.set_locale_choices(ModelRc::from(Rc::new(VecModel::from(
                                        choices
                                            .into_iter()
                                            .map(slint::SharedString::from)
                                            .collect::<Vec<_>>(),
                                    ))));
                                    ui.set_language_index(0);
                                    ui.set_can_clean_flatpak(has_flatpak);
                                    ui.set_clean_flatpak(false);
                                    ui.set_flatpak_savings(flatpak_savings_str.into());
                                    ui.set_package_tab(1);
                                    ui.set_search_text("".into());
                                    ui.set_sort_column(0);
                                    ui.set_sort_ascending(true);

                                    let pkgs = all_packages_ui.lock().unwrap();
                                    let removals = selected_removals_ui.lock().unwrap();
                                    let reps = replaced_folders_ui.lock().unwrap();
                                    apply_package_filter(
                                        &ui, &pkgs, &removals, &reps, 1, "", 0, true,
                                    );
                                    ui.set_status_bar_text(
                                        "Đã nạp ảnh ISO hoàn tất. Sẵn sàng tùy biến.".into(),
                                    );
                                }
                            });
                        }
                        Err(e) => {
                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    let mut cur_log = ui.get_log_text().to_string();
                                    cur_log += &format!("❌ Lỗi nạp ISO: {}\n", e);
                                    ui.set_log_text(cur_log.into());
                                    ui.set_status_bar_text(format!("Lỗi: {}", e).into());
                                }
                            });
                        }
                    }
                });
            }
        });
    }

    // 2. Extract ISO
    {
        let ui_weak = ui_handle.clone();
        let current_image = current_image.clone();
        main_window.on_extract_iso_clicked(move || {
            let cur = current_image.lock().unwrap().clone();
            if let Some(img) = cur {
                let dir_opt = dialog::FileDialog::new().pick_folder();
                if let Some(dest_dir) = dir_opt {
                    let ui_weak = ui_weak.clone();
                    std::thread::spawn(move || {
                        let log_cb = {
                            let ui_weak = ui_weak.clone();
                            move |msg: &str| {
                                let msg_str = msg.to_string();
                                let ui_weak = ui_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_weak.upgrade() {
                                        let mut cur_log = ui.get_log_text().to_string();
                                        cur_log += &format!("{}\n", msg_str);
                                        ui.set_log_text(cur_log.into());
                                        ui.set_status_bar_text(msg_str.into());
                                    }
                                });
                            }
                        };
                        let _ = extract_iso(&img.path, &dest_dir, log_cb);
                    });
                }
            }
        });
    }

    // 3. Add Deb/Pkg
    {
        let ui_weak = ui_handle.clone();
        let staged_debs = staged_debs.clone();
        main_window.on_add_deb_clicked(move || {
            let files = dialog::FileDialog::new()
                .add_filter("Gói Debian", &["deb"])
                .pick_files();

            if let Some(paths) = files {
                let mut deb_items: Vec<AddedPackageItem> = Vec::new();
                let mut debs = staged_debs.lock().unwrap();

                for p in paths {
                    let file_size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                    let file_name = p
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .to_string();
                    deb_items.push(AddedPackageItem {
                        name: file_name.clone().into(),
                        file_name: file_name.into(),
                        size_str: format_bytes(file_size).into(),
                        path: p.display().to_string().into(),
                        status: "Hợp lệ".into(),
                    });
                    debs.push(p);
                }

                if let Some(ui) = ui_weak.upgrade() {
                    let old_model = ui.get_added_packages();
                    let mut all_items = Vec::new();
                    for i in 0..old_model.row_count() {
                        if let Some(item) = old_model.row_data(i) {
                            all_items.push(item);
                        }
                    }
                    all_items.extend(deb_items);
                    ui.set_added_packages(ModelRc::from(Rc::new(VecModel::from(all_items))));
                    ui.set_status_bar_text("Đã thêm gói vào danh sách nhúng.".into());
                }
            }
        });
    }

    // 4. Remove Deb
    {
        let ui_weak = ui_handle.clone();
        let staged_debs = staged_debs.clone();
        main_window.on_remove_deb_clicked(move |idx| {
            let mut debs = staged_debs.lock().unwrap();
            let index = idx as usize;
            if index < debs.len() {
                debs.remove(index);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let old_model = ui.get_added_packages();
                let mut all_items = Vec::new();
                for i in 0..old_model.row_count() {
                    if i != index {
                        if let Some(item) = old_model.row_data(i) {
                            all_items.push(item);
                        }
                    }
                }
                ui.set_added_packages(ModelRc::from(Rc::new(VecModel::from(all_items))));
            }
        });
    }

    // 5. Clear Debs
    {
        let ui_weak = ui_handle.clone();
        let staged_debs = staged_debs.clone();
        main_window.on_clear_debs_clicked(move || {
            staged_debs.lock().unwrap().clear();
            if let Some(ui) = ui_weak.upgrade() {
                ui.set_added_packages(ModelRc::from(Rc::new(VecModel::from(Vec::<
                    AddedPackageItem,
                >::new(
                )))));
            }
        });
    }

    // Filter packages (tabs & search query)
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_filter_packages(move |tab, query| {
            if let Some(ui) = ui_weak.upgrade() {
                let pkgs = all_packages.lock().unwrap();
                let removals = selected_removals.lock().unwrap();
                let reps = replaced_folders.lock().unwrap();
                let sc = *sort_column.lock().unwrap();
                let sa = *sort_ascending.lock().unwrap();
                apply_package_filter(&ui, &pkgs, &removals, &reps, tab, &query, sc, sa);
            }
        });
    }

    // Sort packages
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_sort_packages(move |col| {
            if let Some(ui) = ui_weak.upgrade() {
                let mut c = sort_column.lock().unwrap();
                let mut a = sort_ascending.lock().unwrap();
                if *c == col {
                    *a = !*a;
                } else {
                    *c = col;
                    *a = true;
                }
                ui.set_sort_column(*c);
                ui.set_sort_ascending(*a);
                let pkgs = all_packages.lock().unwrap();
                let removals = selected_removals.lock().unwrap();
                let reps = replaced_folders.lock().unwrap();
                apply_package_filter(
                    &ui,
                    &pkgs,
                    &removals,
                    &reps,
                    ui.get_package_tab(),
                    &ui.get_search_text(),
                    *c,
                    *a,
                );
            }
        });
    }

    // 6. Package Toggled
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_package_toggled(move |idx, checked| {
            if let Some(ui) = ui_weak.upgrade() {
                let model = ui.get_packages();
                let index = idx as usize;
                if let Some(item) = model.row_data(index) {
                    if !item.is_protected {
                        let pkg_name = item.name.to_string();
                        {
                            let mut removals = selected_removals.lock().unwrap();
                            if checked {
                                removals.insert(pkg_name);
                            } else {
                                removals.remove(&pkg_name);
                            }
                        }
                        let pkgs = all_packages.lock().unwrap();
                        let removals = selected_removals.lock().unwrap();
                        let reps = replaced_folders.lock().unwrap();
                        let sc = *sort_column.lock().unwrap();
                        let sa = *sort_ascending.lock().unwrap();
                        apply_package_filter(
                            &ui,
                            &pkgs,
                            &removals,
                            &reps,
                            ui.get_package_tab(),
                            &ui.get_search_text(),
                            sc,
                            sa,
                        );
                    }
                }
            }
        });
    }

    // 7. Select all packages
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_select_all_packages(move |checked| {
            if let Some(ui) = ui_weak.upgrade() {
                let model = ui.get_packages();
                {
                    let mut removals = selected_removals.lock().unwrap();
                    for i in 0..model.row_count() {
                        if let Some(item) = model.row_data(i) {
                            if !item.is_protected {
                                if checked {
                                    removals.insert(item.name.to_string());
                                } else {
                                    removals.remove(&item.name.to_string());
                                }
                            }
                        }
                    }
                }
                let pkgs = all_packages.lock().unwrap();
                let removals = selected_removals.lock().unwrap();
                let reps = replaced_folders.lock().unwrap();
                let sc = *sort_column.lock().unwrap();
                let sa = *sort_ascending.lock().unwrap();
                apply_package_filter(
                    &ui,
                    &pkgs,
                    &removals,
                    &reps,
                    ui.get_package_tab(),
                    &ui.get_search_text(),
                    sc,
                    sa,
                );
            }
        });
    }

    // Replace package with folder
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_replace_package_clicked(move |pkg_name| {
            let pkg_str = pkg_name.to_string();
            let dir_opt = dialog::FileDialog::new()
                .set_title(&format!(
                    "Chọn thư mục chứa tệp thay thế cho gói {}",
                    pkg_str
                ))
                .pick_folder();
            if let Some(dir) = dir_opt {
                {
                    let mut reps = replaced_folders.lock().unwrap();
                    reps.insert(pkg_str.clone(), dir.clone());
                }
                if let Some(ui) = ui_weak.upgrade() {
                    let pkgs = all_packages.lock().unwrap();
                    let removals = selected_removals.lock().unwrap();
                    let reps = replaced_folders.lock().unwrap();
                    let sc = *sort_column.lock().unwrap();
                    let sa = *sort_ascending.lock().unwrap();
                    apply_package_filter(
                        &ui,
                        &pkgs,
                        &removals,
                        &reps,
                        ui.get_package_tab(),
                        &ui.get_search_text(),
                        sc,
                        sa,
                    );
                    let mut cur_log = ui.get_log_text().to_string();
                    cur_log += &format!(
                        "Đã đặt thư mục thay thế cho {}: {}\n",
                        pkg_str,
                        dir.display()
                    );
                    ui.set_log_text(cur_log.into());
                    ui.set_status_bar_text(
                        format!("Đã gán thư mục thay thế cho gói {}", pkg_str).into(),
                    );
                }
            }
        });
    }

    // Reset package replacement
    {
        let ui_weak = ui_handle.clone();
        let all_packages = all_packages.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();
        let sort_column = sort_column.clone();
        let sort_ascending = sort_ascending.clone();
        main_window.on_reset_package_clicked(move |pkg_name| {
            let pkg_str = pkg_name.to_string();
            {
                let mut reps = replaced_folders.lock().unwrap();
                reps.remove(&pkg_str);
            }
            if let Some(ui) = ui_weak.upgrade() {
                let pkgs = all_packages.lock().unwrap();
                let removals = selected_removals.lock().unwrap();
                let reps = replaced_folders.lock().unwrap();
                let sc = *sort_column.lock().unwrap();
                let sa = *sort_ascending.lock().unwrap();
                apply_package_filter(
                    &ui,
                    &pkgs,
                    &removals,
                    &reps,
                    ui.get_package_tab(),
                    &ui.get_search_text(),
                    sc,
                    sa,
                );
                let mut cur_log = ui.get_log_text().to_string();
                cur_log += &format!("Đã hủy thay thế cho gói {}\n", pkg_str);
                ui.set_log_text(cur_log.into());
                ui.set_status_bar_text(format!("Đã hủy thay thế cho gói {}", pkg_str).into());
            }
        });
    }

    // 8. Export Package Files
    {
        let ui_weak = ui_handle.clone();
        let current_image = current_image.clone();
        main_window.on_export_package_clicked(move |pkg_name| {
            let cur = current_image.lock().unwrap().clone();
            if let Some(img) = cur {
                let dest_opt = dialog::FileDialog::new().pick_folder();
                if let Some(dest_dir) = dest_opt {
                    let ui_weak = ui_weak.clone();
                    let pkg_str = pkg_name.to_string();
                    std::thread::spawn(move || {
                        let log_cb = {
                            let ui_weak = ui_weak.clone();
                            move |msg: &str| {
                                let msg_str = msg.to_string();
                                let ui_weak = ui_weak.clone();
                                let _ = slint::invoke_from_event_loop(move || {
                                    if let Some(ui) = ui_weak.upgrade() {
                                        let mut cur_log = ui.get_log_text().to_string();
                                        cur_log += &format!("{}\n", msg_str);
                                        ui.set_log_text(cur_log.into());
                                    }
                                });
                            }
                        };
                        let _ = export_package_files(&img, &pkg_str, &dest_dir, log_cb);
                    });
                }
            }
        });
    }

    {
        let weak = ui_handle.clone();
        main_window.on_add_language_debs(move || {
            if let Some(files) = dialog::FileDialog::new()
                .add_filter("Gói ngôn ngữ .deb", &["deb"])
                .pick_files()
            {
                if let Some(ui) = weak.upgrade() {
                    let mut paths = ui.get_language_deb_paths().to_string();
                    for path in files {
                        if !paths.is_empty() {
                            paths.push('\n');
                        }
                        paths.push_str(&path.to_string_lossy());
                    }
                    ui.set_language_deb_paths(paths.into());
                }
            }
        });
    }

    // 12. Build ISO Action
    {
        let ui_weak = ui_handle.clone();
        let current_image = current_image.clone();
        let staged_debs = staged_debs.clone();
        let selected_removals = selected_removals.clone();
        let replaced_folders = replaced_folders.clone();

        main_window.on_build_iso_clicked(move || {
            let cur = current_image.lock().unwrap().clone();
            if let Some(img) = cur {
                let dest_opt = dialog::FileDialog::new()
                    .add_filter("Ảnh đĩa ISO", &["iso"])
                    .set_file_name("custom-linux.iso")
                    .save_file();

                if let Some(dest_iso) = dest_opt {
                    let ui_weak = ui_weak.clone();
                    let staged_debs = staged_debs.clone();

                    if let Some(ui) = ui_weak.upgrade() {
                        ui.set_is_building(true);
                        ui.set_build_progress(0);
                        ui.set_build_status("Đang chuẩn bị gói dữ liệu...".into());
                        ui.set_current_view(2); // Jump to options/build view

                        // Collect selected packages to remove
                        let remove_set = selected_removals.lock().unwrap().clone();
                        let replace_map = replaced_folders.lock().unwrap().clone();

                        let clean_flatpak = ui.get_clean_flatpak();
                        let rel_name = ui.get_iso_release_text().to_string();

                        let plan = Plan {
                            original_release: img.release.clone(),
                            image: Image {
                                release: rel_name,
                                ..img
                            },
                            remove: remove_set,
                            debs: staged_debs.lock().unwrap().clone(),
                            locales: ui
                                .get_extra_locales()
                                .split(',')
                                .map(str::trim)
                                .filter(|s| !s.is_empty())
                                .map(str::to_owned)
                                .collect(),
                            default_locale: if ui.get_language_index() > 0 {
                                ui.get_locale_choices()
                                    .row_data(ui.get_language_index() as usize)
                                    .map(|s| s.to_string())
                            } else {
                                None
                            },
                            language_debs: ui
                                .get_language_deb_paths()
                                .split('\n')
                                .filter(|s| !s.is_empty())
                                .map(PathBuf::from)
                                .collect(),
                            clean_flatpak,
                            replaced_folders: replace_map,
                        };

                        std::thread::spawn(move || {
                            let log_cb = {
                                let ui_weak = ui_weak.clone();
                                move |msg: &str| {
                                    let msg_str = msg.to_string();
                                    let ui_weak = ui_weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(ui) = ui_weak.upgrade() {
                                            let mut cur_log = ui.get_log_text().to_string();
                                            cur_log += &format!("{}\n", msg_str);
                                            ui.set_log_text(cur_log.into());
                                        }
                                    });
                                }
                            };

                            let progress_cb = {
                                let ui_weak = ui_weak.clone();
                                move |pct: i32, msg: &str| {
                                    let msg_str = msg.to_string();
                                    let ui_weak = ui_weak.clone();
                                    let _ = slint::invoke_from_event_loop(move || {
                                        if let Some(ui) = ui_weak.upgrade() {
                                            ui.set_build_progress(pct);
                                            ui.set_build_status(msg_str.into());
                                        }
                                    });
                                }
                            };

                            let res = build_iso(&plan, &dest_iso, log_cb, progress_cb);

                            let _ = slint::invoke_from_event_loop(move || {
                                if let Some(ui) = ui_weak.upgrade() {
                                    ui.set_is_building(false);
                                    match res {
                                        Ok(()) => {
                                            ui.set_build_progress(100);
                                            ui.set_build_status("Hoàn tất tạo ISO!".into());
                                            ui.set_status_bar_text(
                                                "Tạo ảnh ISO tùy chỉnh thành công!".into(),
                                            );
                                        }
                                        Err(e) => {
                                            ui.set_build_status(format!("Lỗi: {}", e).into());
                                            ui.set_status_bar_text(
                                                format!("Đóng gói thất bại: {}", e).into(),
                                            );
                                        }
                                    }
                                }
                            });
                        });
                    }
                }
            }
        });
    }

    // Window controls for custom Slint titlebar
    {
        let ui_weak = main_window.as_weak();
        main_window.on_minimize_window(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().set_minimized(true);
            }
        });
    }

    {
        let ui_weak = main_window.as_weak();
        main_window.on_maximize_window(move || {
            if let Some(ui) = ui_weak.upgrade() {
                ui.window().set_maximized(!ui.window().is_maximized());
            }
        });
    }

    main_window.on_close_window(|| {
        std::process::exit(0);
    });

    // Center window on screen at startup
    extern "system" {
        fn GetSystemMetrics(nIndex: i32) -> i32;
    }

    main_window.show()?;

    #[cfg(target_os = "windows")]
    {
        win32_icon::apply_taskbar_icon();
        slint::Timer::single_shot(std::time::Duration::from_millis(250), || {
            win32_icon::apply_taskbar_icon();
        });
    }

    let scale = main_window.window().scale_factor();
    let win_w = (1260.0 * scale) as i32;
    let win_h = (760.0 * scale) as i32;
    let screen_w = unsafe { GetSystemMetrics(0) }; // SM_CXSCREEN
    let screen_h = unsafe { GetSystemMetrics(1) }; // SM_CYSCREEN
    if screen_w > win_w && screen_h > win_h {
        let pos_x = (screen_w - win_w) / 2;
        let pos_y = (screen_h - win_h) / 2;
        main_window
            .window()
            .set_position(slint::PhysicalPosition::new(pos_x, pos_y));
    }

    slint::run_event_loop()?;
    Ok(())
}

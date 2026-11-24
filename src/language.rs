use std::collections::BTreeSet;

/// Only glibc locale identifiers are accepted, never shell fragments.
pub fn validate_locale(locale: &str) -> Result<(), String> {
    let base = locale
        .strip_suffix(".UTF-8")
        .ok_or("Locale phải kết thúc bằng .UTF-8")?;
    let (language, territory) = base.split_once('_').ok_or("Dùng locale như vi_VN.UTF-8")?;
    if !(2..=3).contains(&language.len())
        || !language.bytes().all(|b| b.is_ascii_lowercase())
        || territory.len() != 2
        || !territory.bytes().all(|b| b.is_ascii_uppercase())
    {
        return Err(format!("Locale không hợp lệ: {locale}"));
    }
    Ok(())
}

pub fn selected_locales(
    extra: &[String],
    default: Option<&str>,
) -> Result<BTreeSet<String>, String> {
    let mut locales = BTreeSet::new();
    for locale in extra.iter().map(String::as_str).chain(default) {
        validate_locale(locale)?;
        locales.insert(locale.to_owned());
    }
    Ok(locales)
}

/// Preserve enabled locales and comments already present in the vendor image.
pub fn merge_locale_gen(original: &str, locales: &BTreeSet<String>) -> String {
    let mut output = original.trim_end().to_owned();
    output.push('\n');
    for locale in locales {
        if !original
            .lines()
            .any(|line| line.split_whitespace().next() == Some(locale.as_str()))
        {
            output.push_str(&format!("{locale} UTF-8\n"));
        }
    }
    output
}

pub fn language_priority(locale: &str) -> String {
    let base = locale.trim_end_matches(".UTF-8");
    let language = base.split('_').next().unwrap_or("en");
    if language == "en" {
        format!("{base}:en")
    } else {
        format!("{base}:{language}:en")
    }
}

/// Preserve unrelated environment keys and remove LC_ALL so it cannot override LANG.
pub fn set_language(original: &str, locale: &str) -> String {
    let mut output = String::new();
    for line in original.lines() {
        let key = line
            .trim_start()
            .strip_prefix("export ")
            .unwrap_or(line.trim_start())
            .split('=')
            .next()
            .unwrap_or("");
        if key == "LANG" || key == "LANGUAGE" || key.starts_with("LC_") {
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output.push_str(&format!(
        "LANG={locale}\nLANGUAGE={}\n",
        language_priority(locale)
    ));
    output
}

/// Patch both GRUB's linux lines and ISOLINUX's append lines, including installer entries.
pub fn patch_boot_config(original: &str, locale: &str) -> String {
    let mut output = String::new();
    for line in original.split_inclusive('\n') {
        let trimmed = line.trim_start();
        let command = trimmed.split_whitespace().next().unwrap_or("");
        if !matches!(command, "linux" | "linuxefi" | "append") || !line.contains("boot=live") {
            output.push_str(line);
            continue;
        }
        let mut found = false;
        for token in line.split_inclusive(char::is_whitespace) {
            let value = token.trim_end_matches(char::is_whitespace);
            let whitespace = &token[value.len()..];
            if value.starts_with("locales=") || value.starts_with("locale=") {
                output.push_str(&format!("locales={locale}{whitespace}"));
                found = true;
            } else {
                output.push_str(token);
            }
        }
        if !found {
            if line.ends_with('\n') {
                output.pop();
            }
            output.push_str(&format!(" locales={locale}"));
            if line.ends_with('\n') {
                output.push('\n');
            }
        }
    }
    output
}

pub const SETUP_SCRIPT: &str = include_str!("../pkg/setup-locales.sh");
pub const SETUP_SERVICE: &str = "[Unit]\nDescription=ISOLite offline language setup\nAfter=local-fs.target\nBefore=display-manager.service\nConditionPathExists=!/var/lib/isolite/locales-ready\n\n[Service]\nType=oneshot\nExecStart=/bin/sh /usr/lib/isolite/setup-locales.sh\nTimeoutStartSec=600\nRemainAfterExit=yes\n\n[Install]\nWantedBy=multi-user.target\n";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_shell_injection_and_non_utf8() {
        for bad in [
            "vi_VN.UTF-8;reboot",
            "$(id).UTF-8",
            "../../en_US.UTF-8",
            "en_US",
            "en_us.UTF-8",
            "en_US.UTF-8\n",
        ] {
            assert!(validate_locale(bad).is_err(), "{bad}");
        }
        assert!(validate_locale("vi_VN.UTF-8").is_ok());
    }

    #[test]
    fn preserves_chinese_locale_and_environment() {
        let locales = selected_locales(&["vi_VN.UTF-8".into()], Some("vi_VN.UTF-8")).unwrap();
        let generated = merge_locale_gen("zh_CN.UTF-8 UTF-8\n# vi_VN.UTF-8 UTF-8\n", &locales);
        assert!(generated.contains("zh_CN.UTF-8 UTF-8"));
        assert_eq!(generated.matches("\nvi_VN.UTF-8 UTF-8").count(), 1);
        assert_eq!(
            set_language(
                "PATH=/bin\nLC_ALL=zh_CN.UTF-8\nexport LANG=zh_CN.UTF-8\n",
                "vi_VN.UTF-8"
            ),
            "PATH=/bin\nLANG=vi_VN.UTF-8\nLANGUAGE=vi_VN:vi:en\n"
        );
    }

    #[test]
    fn patches_live_and_installer_boot_without_changing_other_entries() {
        let original = "# locales=zh_CN.UTF-8\n linux /live/vmlinuz boot=live locales=zh_CN.UTF-8 quiet\n append boot=live only-installer\n linux /vmlinuz root=/dev/sda1\n";
        let patched = patch_boot_config(original, "en_US.UTF-8");
        assert!(patched.contains("boot=live locales=en_US.UTF-8 quiet"));
        assert!(patched.contains("boot=live only-installer locales=en_US.UTF-8"));
        assert!(patched.contains("linux /vmlinuz root=/dev/sda1\n"));
        assert!(patched.starts_with("# locales=zh_CN.UTF-8"));
        assert_eq!(patch_boot_config(&patched, "en_US.UTF-8"), patched);
    }
}

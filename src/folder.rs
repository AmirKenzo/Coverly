use anyhow::{bail, Context, Result};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub const ICON_PREFIX: &str = "coverly-icon-";

pub fn is_our_icon(name: &str) -> bool {
    name.starts_with(ICON_PREFIX) && name.to_ascii_lowercase().ends_with(".ico")
}

pub fn apply_icon(folder: &Path, ico: &[u8]) -> Result<PathBuf> {
    if !folder.is_dir() {
        bail!("not a folder: {}", folder.display());
    }
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let name = format!("{ICON_PREFIX}{:08x}-{stamp}.ico", fnv(ico));

    let icon_path = folder.join(&name);
    fs::write(&icon_path, ico).with_context(|| format!("writing {}", icon_path.display()))?;

    if !attrs::set_folder_icon(folder, &name) {
        let ini = folder.join("desktop.ini");
        let new = set_icon(&read_ini(&ini), Some(&name));
        attrs::clear(&ini);
        write_ini(&ini, &new).with_context(|| format!("writing {}", ini.display()))?;
        attrs::hide(&ini);
        attrs::mark_folder(folder, true);
    } else {
        attrs::hide(&folder.join("desktop.ini"));
    }
    attrs::hide(&icon_path);

    remove_our_icons(folder, Some(&name))?;
    attrs::refresh(folder);
    Ok(icon_path)
}

pub fn remove_icon(folder: &Path) -> Result<()> {
    let ini = folder.join("desktop.ini");
    if ini.exists() {
        let new = set_icon(&read_ini(&ini), None);
        attrs::clear(&ini);
        if is_empty_ini(&new) {
            fs::remove_file(&ini).with_context(|| format!("removing {}", ini.display()))?;
            attrs::mark_folder(folder, false);
        } else {
            write_ini(&ini, &new)?;
            attrs::hide(&ini);
        }
    }
    remove_our_icons(folder, None)?;
    attrs::refresh(folder);
    Ok(())
}

fn remove_our_icons(folder: &Path, keep: Option<&str>) -> Result<()> {
    for e in fs::read_dir(folder)?.flatten() {
        let n = e.file_name().to_string_lossy().to_string();
        if is_our_icon(&n) && Some(n.as_str()) != keep {
            attrs::clear(&e.path());
            let _ = fs::remove_file(e.path());
        }
    }
    Ok(())
}

fn read_ini(p: &Path) -> String {
    match fs::read(p) {
        Ok(b) => decode(&b),
        Err(_) => String::new(),
    }
}

fn decode(b: &[u8]) -> String {
    if b.len() >= 2 && b[0] == 0xFF && b[1] == 0xFE {
        let u: Vec<u16> = b[2..]
            .chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16_lossy(&u)
    } else {
        const BOM: [u8; 3] = [0xEF, 0xBB, 0xBF];
        let s = if b.starts_with(&BOM) { &b[3..] } else { b };
        String::from_utf8_lossy(s).into_owned()
    }
}

fn write_ini(p: &Path, s: &str) -> Result<()> {
    let mut out = vec![0xFF, 0xFE];
    for u in s.encode_utf16() {
        out.extend_from_slice(&u.to_le_bytes());
    }
    fs::write(p, out)?;
    Ok(())
}

pub fn set_icon(content: &str, icon: Option<&str>) -> String {
    let mut out: Vec<String> = Vec::new();
    let mut in_sci = false;
    let mut found = false;
    for line in content.lines() {
        let t = line.trim();
        if t.starts_with('[') {
            in_sci = t.eq_ignore_ascii_case("[.ShellClassInfo]");
            if in_sci {
                found = true;
                out.push(line.to_string());
                if let Some(i) = icon {
                    out.push(format!("IconResource={i},0"));
                }
                continue;
            }
        }
        if in_sci {
            let key = t.split('=').next().unwrap_or("").trim().to_ascii_lowercase();
            if key == "iconresource" || key == "iconfile" || key == "iconindex" {
                continue;
            }
        }
        out.push(line.to_string());
    }
    if !found {
        if let Some(i) = icon {
            out.insert(0, format!("IconResource={i},0"));
            out.insert(0, "[.ShellClassInfo]".to_string());
        }
    }
    let mut s = out.join("\r\n");
    s.push_str("\r\n");
    s
}

fn is_empty_ini(s: &str) -> bool {
    s.lines()
        .map(str::trim)
        .all(|l| l.is_empty() || (l.starts_with('[') && l.ends_with(']')))
}

fn fnv(b: &[u8]) -> u32 {
    let mut h: u32 = 0x811c_9dc5;
    for x in b {
        h ^= *x as u32;
        h = h.wrapping_mul(0x0100_0193);
    }
    h
}

#[cfg(windows)]
mod attrs {
    use std::{os::windows::ffi::OsStrExt, path::Path};
    use windows_sys::Win32::Storage::FileSystem::{
        GetFileAttributesW, SetFileAttributesW, FILE_ATTRIBUTE_HIDDEN, FILE_ATTRIBUTE_NORMAL, FILE_ATTRIBUTE_READONLY,
        FILE_ATTRIBUTE_SYSTEM, INVALID_FILE_ATTRIBUTES,
    };
    use windows_sys::Win32::UI::Shell::{
        SHChangeNotify, SHGetSetFolderCustomSettings, FCSM_ICONFILE, SHCNE_ASSOCCHANGED, SHCNE_UPDATEDIR,
        SHCNE_UPDATEITEM, SHCNF_FLUSH, SHCNF_IDLIST, SHCNF_PATHW, SHFOLDERCUSTOMSETTINGS,
    };

    const FCS_FORCEWRITE: u32 = 2;

    fn wide(p: &Path) -> Vec<u16> {
        p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
    }

    fn get(p: &Path) -> Option<u32> {
        let w = wide(p);
        let a = unsafe { GetFileAttributesW(w.as_ptr()) };
        (a != INVALID_FILE_ATTRIBUTES).then_some(a)
    }

    fn set(p: &Path, a: u32) {
        let w = wide(p);
        unsafe {
            SetFileAttributesW(w.as_ptr(), a);
        }
    }

    pub fn clear(p: &Path) {
        if get(p).is_some() {
            set(p, FILE_ATTRIBUTE_NORMAL);
        }
    }

    pub fn hide(p: &Path) {
        if get(p).is_some() {
            set(p, FILE_ATTRIBUTE_HIDDEN | FILE_ATTRIBUTE_SYSTEM);
        }
    }

    pub fn mark_folder(p: &Path, on: bool) {
        if let Some(a) = get(p) {
            let n = if on {
                a | FILE_ATTRIBUTE_READONLY
            } else {
                a & !FILE_ATTRIBUTE_READONLY
            };
            set(p, n);
        }
    }

    pub fn set_folder_icon(folder: &Path, icon_file: &str) -> bool {
        let mut icon: Vec<u16> = icon_file.encode_utf16().chain(std::iter::once(0)).collect();
        let path = wide(folder);
        let mut s: SHFOLDERCUSTOMSETTINGS = unsafe { std::mem::zeroed() };
        s.dwSize = std::mem::size_of::<SHFOLDERCUSTOMSETTINGS>() as u32;
        s.dwMask = FCSM_ICONFILE;
        s.pszIconFile = icon.as_mut_ptr();
        s.cchIconFile = 0;
        s.iIconIndex = 0;
        let hr = unsafe { SHGetSetFolderCustomSettings(&mut s, path.as_ptr(), FCS_FORCEWRITE) };
        hr >= 0
    }

    pub fn refresh(p: &Path) {
        let w = wide(p);
        unsafe {
            SHChangeNotify(
                SHCNE_UPDATEITEM as _,
                (SHCNF_PATHW | SHCNF_FLUSH) as _,
                w.as_ptr() as *const _,
                std::ptr::null(),
            );
            SHChangeNotify(
                SHCNE_UPDATEDIR as _,
                (SHCNF_PATHW | SHCNF_FLUSH) as _,
                w.as_ptr() as *const _,
                std::ptr::null(),
            );
            if let Some(parent) = p.parent() {
                let pw = wide(parent);
                SHChangeNotify(
                    SHCNE_UPDATEDIR as _,
                    (SHCNF_PATHW | SHCNF_FLUSH) as _,
                    pw.as_ptr() as *const _,
                    std::ptr::null(),
                );
            }
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as _,
                SHCNF_IDLIST as _,
                std::ptr::null(),
                std::ptr::null(),
            );
        }
    }
}

#[cfg(not(windows))]
mod attrs {
    use std::path::Path;
    pub fn clear(_: &Path) {}
    pub fn hide(_: &Path) {}
    pub fn mark_folder(_: &Path, _: bool) {}
    pub fn set_folder_icon(_: &Path, _: &str) -> bool {
        false
    }
    pub fn refresh(_: &Path) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_section_when_missing() {
        let s = set_icon("", Some("a.ico"));
        assert_eq!(s, "[.ShellClassInfo]\r\nIconResource=a.ico,0\r\n");
    }

    #[test]
    fn replaces_old_icon_and_keeps_other_lines() {
        let old = "[.ShellClassInfo]\r\nIconFile=old.ico\r\nIconIndex=0\r\nInfoTip=hi\r\n[ViewState]\r\nMode=\r\n";
        let s = set_icon(old, Some("new.ico"));
        assert!(s.contains("IconResource=new.ico,0"));
        assert!(!s.contains("old.ico"));
        assert!(s.contains("InfoTip=hi"));
        assert!(s.contains("[ViewState]"));
    }

    #[test]
    fn removing_leaves_empty_file() {
        let s = set_icon("[.ShellClassInfo]\r\nIconResource=x.ico,0\r\n", None);
        assert!(is_empty_ini(&s));
    }
}

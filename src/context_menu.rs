#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    User,
    Machine,
}

pub use imp::{register, registered, unregister};

/// Handles `--register-menu` and `--unregister-menu` from the installer. Returns the exit code when one of them ran.
pub fn run_cli() -> Option<i32> {
    let arg = std::env::args().nth(1)?;
    let done = match arg.as_str() {
        "--register-menu" => register(Scope::Machine),
        "--unregister-menu" => unregister(Scope::Machine).and_then(|_| unregister(Scope::User)),
        _ => return None,
    };
    Some(if done.is_ok() { 0 } else { 1 })
}

#[cfg(windows)]
mod imp {
    use super::Scope;
    use crate::videos::EXTS;
    use anyhow::{bail, Result};
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt};
    use windows_sys::Win32::{
        System::Registry::{
            RegCloseKey, RegCreateKeyExW, RegDeleteTreeW, RegOpenKeyExW, RegSetValueExW, HKEY, HKEY_CURRENT_USER,
            HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_DWORD, REG_OPTION_NON_VOLATILE, REG_SZ,
        },
        UI::Shell::{SHChangeNotify, SHCNE_ASSOCCHANGED, SHCNF_IDLIST},
    };

    const FOLDER_KEY: &str = r"Software\Classes\Directory\shell\Coverly";
    const ECF_SEPARATORBEFORE: u32 = 0x20;

    enum Value<'a> {
        Str(&'a str),
        Dword(u32),
    }

    fn wide(s: &str) -> Vec<u16> {
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    }

    fn root(scope: Scope) -> HKEY {
        match scope {
            Scope::User => HKEY_CURRENT_USER,
            Scope::Machine => HKEY_LOCAL_MACHINE,
        }
    }

    fn video_key(ext: &str) -> String {
        format!(r"Software\Classes\SystemFileAssociations\.{ext}\shell\Coverly")
    }

    fn set(root: HKEY, path: &str, values: &[(&str, Value)]) -> Result<()> {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe {
            RegCreateKeyExW(
                root,
                wide(path).as_ptr(),
                0,
                std::ptr::null(),
                REG_OPTION_NON_VOLATILE,
                KEY_WRITE,
                std::ptr::null(),
                &mut key,
                std::ptr::null_mut(),
            )
        };
        if status != 0 {
            bail!("couldn't write the registry key {path} (error {status})");
        }
        let mut result = Ok(());
        for (name, value) in values {
            let name = wide(name);
            let status = match value {
                Value::Str(s) => {
                    let data = wide(s);
                    unsafe {
                        RegSetValueExW(
                            key,
                            name.as_ptr(),
                            0,
                            REG_SZ,
                            data.as_ptr().cast(),
                            (data.len() * 2) as u32,
                        )
                    }
                }
                Value::Dword(d) => unsafe {
                    RegSetValueExW(key, name.as_ptr(), 0, REG_DWORD, (d as *const u32).cast(), 4)
                },
            };
            if status != 0 {
                result = Err(anyhow::anyhow!(
                    "couldn't write the registry key {path} (error {status})"
                ));
                break;
            }
        }
        unsafe { RegCloseKey(key) };
        result
    }

    fn delete(root: HKEY, path: &str) {
        unsafe { RegDeleteTreeW(root, wide(path).as_ptr()) };
    }

    fn notify() {
        unsafe {
            SHChangeNotify(
                SHCNE_ASSOCCHANGED as i32,
                SHCNF_IDLIST,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
    }

    pub fn registered(scope: Scope) -> bool {
        let mut key: HKEY = std::ptr::null_mut();
        let status = unsafe { RegOpenKeyExW(root(scope), wide(FOLDER_KEY).as_ptr(), 0, KEY_READ, &mut key) };
        if status == 0 {
            unsafe { RegCloseKey(key) };
        }
        status == 0
    }

    pub fn register(scope: Scope) -> Result<()> {
        let exe = std::env::current_exe()?.display().to_string();
        let icon = format!("\"{exe}\",0");
        let command = |flag: &str| format!("\"{exe}\" {flag} \"%1\"");
        let r = root(scope);

        set(
            r,
            FOLDER_KEY,
            &[
                ("MUIVerb", Value::Str("Coverly")),
                ("Icon", Value::Str(&icon)),
                ("SubCommands", Value::Str("")),
                ("MultiSelectModel", Value::Str("Player")),
            ],
        )?;
        let items = [
            ("1pick", "Pick covers in Coverly", "--pick", 0),
            ("2auto", "Set covers automatically", "--auto", 0),
            ("3remove", "Remove folder icon", "--remove", ECF_SEPARATORBEFORE),
        ];
        for (key, label, flag, flags) in items {
            let path = format!(r"{FOLDER_KEY}\shell\{key}");
            set(
                r,
                &path,
                &[
                    ("MUIVerb", Value::Str(label)),
                    ("MultiSelectModel", Value::Str("Player")),
                    ("CommandFlags", Value::Dword(flags)),
                ],
            )?;
            set(r, &format!(r"{path}\command"), &[("", Value::Str(&command(flag)))])?;
        }

        for ext in EXTS {
            let path = video_key(ext);
            set(
                r,
                &path,
                &[
                    ("MUIVerb", Value::Str("Make folders with Coverly")),
                    ("Icon", Value::Str(&icon)),
                    ("MultiSelectModel", Value::Str("Player")),
                ],
            )?;
            set(
                r,
                &format!(r"{path}\command"),
                &[("", Value::Str(&command("--videos")))],
            )?;
        }
        notify();
        Ok(())
    }

    pub fn unregister(scope: Scope) -> Result<()> {
        let r = root(scope);
        delete(r, FOLDER_KEY);
        for ext in EXTS {
            delete(r, &video_key(ext));
        }
        notify();
        Ok(())
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Scope;
    use anyhow::{bail, Result};

    pub fn registered(_scope: Scope) -> bool {
        false
    }

    pub fn register(_scope: Scope) -> Result<()> {
        bail!("the right-click menu is only on Windows")
    }

    pub fn unregister(_scope: Scope) -> Result<()> {
        Ok(())
    }
}

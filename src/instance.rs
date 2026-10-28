use eframe::egui;
use serde::{Deserialize, Serialize};
use std::{
    ffi::OsString,
    path::PathBuf,
    sync::{
        mpsc::{channel, Receiver},
        Arc, OnceLock,
    },
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Action {
    #[default]
    Pick,
    Auto,
    Videos,
    Remove,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Request {
    pub action: Action,
    pub paths: Vec<PathBuf>,
}

impl Request {
    pub fn from_args(args: impl Iterator<Item = OsString>) -> Option<Self> {
        let mut action = Action::Pick;
        let mut paths = Vec::new();
        for a in args {
            match a.to_str() {
                Some("--pick") => action = Action::Pick,
                Some("--auto") => action = Action::Auto,
                Some("--videos") => action = Action::Videos,
                Some("--remove") => action = Action::Remove,
                _ => {
                    let p = PathBuf::from(a);
                    if p.exists() {
                        paths.push(p);
                    }
                }
            }
        }
        (!paths.is_empty()).then_some(Self { action, paths })
    }
}

pub struct Inbox {
    rx: Receiver<Request>,
    ctx: Arc<OnceLock<egui::Context>>,
}

impl Inbox {
    pub fn attach(&self, ctx: &egui::Context) {
        let _ = self.ctx.set(ctx.clone());
    }

    pub fn try_recv(&self) -> Option<Request> {
        self.rx.try_recv().ok()
    }
}

pub use imp::{claim, send, Server};

impl Server {
    /// Reads requests from later launches on a background thread for as long as the app runs.
    pub fn listen(self) -> Inbox {
        let (tx, rx) = channel();
        let ctx: Arc<OnceLock<egui::Context>> = Arc::default();
        let repaint = ctx.clone();
        std::thread::spawn(move || {
            self.serve(|req| {
                let _ = tx.send(req);
                if let Some(c) = repaint.get() {
                    c.request_repaint();
                }
            })
        });
        Inbox { rx, ctx }
    }
}

#[cfg(windows)]
mod imp {
    use super::Request;
    use std::{
        fs::{File, OpenOptions},
        io::{Read, Write},
        os::windows::{
            ffi::OsStrExt,
            io::{AsRawHandle, FromRawHandle, OwnedHandle},
        },
        time::{Duration, Instant},
    };
    use windows_sys::Win32::{
        Foundation::{GetLastError, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE},
        Storage::FileSystem::{FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND},
        System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS, PIPE_TYPE_BYTE,
            PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        },
    };

    pub struct Server(OwnedHandle);

    fn pipe_name() -> String {
        let user: String = std::env::var("USERNAME")
            .unwrap_or_default()
            .chars()
            .filter(|c| c.is_ascii_alphanumeric())
            .collect();
        format!(r"\\.\pipe\Coverly-{user}")
    }

    fn create(first: bool) -> Option<OwnedHandle> {
        let name: Vec<u16> = std::ffi::OsStr::new(&pipe_name())
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let flags = PIPE_ACCESS_INBOUND | if first { FILE_FLAG_FIRST_PIPE_INSTANCE } else { 0 };
        let h = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                flags,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_UNLIMITED_INSTANCES,
                0,
                64 * 1024,
                0,
                std::ptr::null(),
            )
        };
        (h != INVALID_HANDLE_VALUE).then(|| unsafe { OwnedHandle::from_raw_handle(h) })
    }

    /// Only one process can create the first instance of the pipe, so this decides which launch owns the window.
    pub fn claim() -> Option<Server> {
        create(true).map(Server)
    }

    impl Server {
        pub(super) fn serve(self, on_request: impl Fn(Request)) {
            let mut current = self.0;
            loop {
                let connected = unsafe { ConnectNamedPipe(current.as_raw_handle(), std::ptr::null_mut()) } != 0
                    || unsafe { GetLastError() } == ERROR_PIPE_CONNECTED;
                let next = loop {
                    if let Some(h) = create(false) {
                        break h;
                    }
                    std::thread::sleep(Duration::from_millis(200));
                };
                if connected {
                    let mut body = String::new();
                    if File::from(current).read_to_string(&mut body).is_ok() {
                        if let Ok(req) = serde_json::from_str(&body) {
                            on_request(req);
                        }
                    }
                }
                current = next;
            }
        }
    }

    /// Hands the request to the running window. Returns false when no window took it.
    pub fn send(req: &Request) -> bool {
        let Ok(body) = serde_json::to_vec(req) else {
            return false;
        };
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match OpenOptions::new().write(true).open(pipe_name()) {
                Ok(mut f) => return f.write_all(&body).is_ok() && f.sync_all().is_ok(),
                Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
                Err(_) => return false,
            }
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::Request;

    pub struct Server;

    pub fn claim() -> Option<Server> {
        None
    }

    impl Server {
        pub(super) fn serve(self, _on_request: impl Fn(Request)) {}
    }

    pub fn send(_req: &Request) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_action_and_existing_paths() {
        let here = std::env::current_dir().unwrap();
        let args = ["--auto", here.to_str().unwrap(), "Z:\\no\\such\\folder"].map(OsString::from);
        let r = Request::from_args(args.into_iter()).unwrap();
        assert_eq!(r.action, Action::Auto);
        assert_eq!(r.paths, vec![here.clone()]);

        let r = Request::from_args([OsString::from(&here)].into_iter()).unwrap();
        assert_eq!(r.action, Action::Pick);

        assert!(Request::from_args(["--remove"].map(OsString::from).into_iter()).is_none());
    }
}

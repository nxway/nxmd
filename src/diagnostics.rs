//! Local diagnostics. Each process owns one bounded log for its lifetime.
use std::{
    fs::{self, File, OpenOptions},
    io::{self, BufRead, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};
const COUNT: usize = 10;
const TOTAL: u64 = 10 * 1024 * 1024;
const ROUTINE: u64 = 896 * 1024;
const MAX_AGE: Duration = Duration::from_secs(14 * 86400);
struct Output {
    file: File,
    bytes: u64,
}
struct Log {
    output: Mutex<Output>,
    crash: File,
    verbose: bool,
    panicked: AtomicBool,
}
static LOG: OnceLock<Log> = OnceLock::new();
struct Logger;
static LOGGER: Logger = Logger;
fn directory(path: &Path) -> io::Result<()> {
    match fs::create_dir(path) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e),
    }
    let meta = fs::symlink_metadata(path)?;
    if !meta.is_dir() || linked(&meta) {
        return Err(io::Error::other("Invalid diagnostic directory"));
    }
    Ok(())
}
fn linked(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        m.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}
fn identity(path: &Path) -> Option<(u64, u32)> {
    let (stamp, pid) = path
        .file_name()?
        .to_str()?
        .strip_prefix("nxmd-")?
        .strip_suffix(".log")?
        .split_once('-')?;
    if ![stamp, pid]
        .iter()
        .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
    {
        return None;
    }
    Some((stamp.parse().ok()?, pid.parse().ok()?))
}
fn process_alive(pid: u32) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::{
            Foundation::CloseHandle,
            System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        };
        // Access denied is ambiguous. Keep the log in that case.
        let h = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if h.is_null() {
            return io::Error::last_os_error().raw_os_error() != Some(87);
        }
        unsafe { CloseHandle(h) };
        true
    }
    #[cfg(not(windows))]
    {
        Path::new("/proc").join(pid.to_string()).exists() || pid == std::process::id()
    }
}
fn prune(dir: &Path, now: SystemTime) -> io::Result<()> {
    let mut logs = Vec::new();
    for entry in fs::read_dir(dir)?.flatten() {
        let path = entry.path();
        let Some((stamp, pid)) = identity(&path) else {
            continue;
        };
        let Ok(m) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !m.is_file() || linked(&m) {
            continue;
        }
        let Ok(file) = File::open(&path) else {
            continue;
        };
        let mut first = String::new();
        if io::BufReader::new(file.take(512))
            .read_line(&mut first)
            .is_err()
            || !first.starts_with("SystemTime {")
            || !first.contains(" INFO nxmd::diagnostics: nxmd ")
            || !first.trim_end().ends_with(" started")
        {
            continue;
        }
        let Some(created) = UNIX_EPOCH.checked_add(Duration::from_millis(stamp)) else {
            continue;
        };
        logs.push((!process_alive(pid), created, path, m.len()));
    }
    // Active logs count first, then retain the most recent eligible archives.
    logs.sort_by_key(|(inactive, time, _, _)| (*inactive, std::cmp::Reverse(*time)));
    let (mut count, mut bytes) = (0, 0u64);
    for (inactive, created, path, len) in logs {
        if inactive
            && (now.duration_since(created).unwrap_or_default() > MAX_AGE
                || count >= COUNT
                || bytes.saturating_add(len) > TOTAL)
        {
            if fs::remove_file(path).is_ok() {
                continue;
            }
        }
        count += 1;
        bytes = bytes.saturating_add(len);
    }
    Ok(())
}
impl Log {
    fn open(profile: &Path) -> io::Result<Self> {
        directory(profile)?;
        let dir = profile.join("logs");
        directory(&dir)?;
        let now = SystemTime::now();
        let stamp = now
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let mut opts = OpenOptions::new();
        opts.create_new(true).append(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            opts.share_mode(1 | 2);
        }
        let mut file = opts.open(dir.join(format!("nxmd-{stamp}-{}.log", std::process::id())))?;
        writeln!(
            file,
            "{now:?} INFO nxmd::diagnostics: nxmd {} started",
            env!("CARGO_PKG_VERSION")
        )?;
        let bytes = file.metadata()?.len();
        let crash = file.try_clone()?;
        let _ = prune(&dir, now);
        Ok(Self {
            output: Mutex::new(Output { file, bytes }),
            crash,
            verbose: std::env::var_os("NXMD_DIAGNOSTICS").as_deref()
                == Some(std::ffi::OsStr::new("verbose")),
            panicked: AtomicBool::new(false),
        })
    }
    fn append(&self, message: &str) {
        let Ok(mut out) = self.output.lock() else {
            return;
        };
        let n = message.len() as u64;
        if out.bytes + n <= ROUTINE && out.file.write_all(message.as_bytes()).is_ok() {
            out.bytes += n;
        }
    }
}
impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        LOG.get().is_some_and(|l| {
            m.level() <= log::Level::Warn
                || (l.verbose && m.target().starts_with("nxmd") && m.level() <= log::Level::Info)
        })
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata())
            && let Some(l) = LOG.get()
        {
            let text = if l.verbose {
                r.args().to_string()
            } else {
                r.args()
                    .as_str()
                    .unwrap_or("See operation error in application")
                    .to_owned()
            };
            l.append(&format!(
                "{:?} {} {}:{} {}\n",
                SystemTime::now(),
                r.level(),
                r.target(),
                r.line().unwrap_or(0),
                text
            ));
        }
    }
    fn flush(&self) {}
}
pub fn init() {
    let session = nxmd::config::Session::path();
    if let Some(profile) = session.parent()
        && let Ok(l) = Log::open(profile)
    {
        let _ = LOG.set(l);
    }
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| {
        if let Some(l) = LOG.get()
            && !l.panicked.swap(true, Ordering::Relaxed)
        {
            let reason =
                if info.payload().downcast_ref::<&str>() == Some(&"nxmd diagnostic self-test") {
                    "nxmd diagnostic self-test"
                } else {
                    "Rust panic"
                };
            let trace = format!(
                "PANIC: {reason} at {:?}\n{}\n",
                info.location(),
                std::backtrace::Backtrace::force_capture()
            );
            let mut f = &l.crash;
            let _ = f.write_all(&trace.as_bytes()[..trace.len().min(96 * 1024)]);
            let _ = f.sync_data();
        }
    }));
    #[cfg(windows)]
    unsafe {
        install_native_handler();
    }
}
#[cfg(windows)]
static CRASH_HANDLE: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(windows)]
unsafe fn install_native_handler() {
    use std::os::windows::io::IntoRawHandle;
    if let Some(log) = LOG.get()
        && let Ok(file) = log.crash.try_clone()
    {
        CRASH_HANDLE.store(file.into_raw_handle(), std::sync::atomic::Ordering::Relaxed);
    }
    unsafe {
        windows_sys::Win32::System::Diagnostics::Debug::SetUnhandledExceptionFilter(Some(
            native_exception,
        ));
    }
}
#[cfg(windows)]
unsafe extern "system" fn native_exception(
    info: *const windows_sys::Win32::System::Diagnostics::Debug::EXCEPTION_POINTERS,
) -> i32 {
    // Avoid allocation/locks in a potentially corrupted process. Record the exception code.
    let mut message = *b"UNHANDLED WINDOWS EXCEPTION 0x00000000\r\n";
    if !info.is_null() && unsafe { !(*info).ExceptionRecord.is_null() } {
        let code = unsafe { (*(*info).ExceptionRecord).ExceptionCode } as u32;
        for i in 0..8 {
            message[30 + i] = b"0123456789ABCDEF"[((code >> (28 - i * 4)) & 15) as usize];
        }
    }
    let handle = CRASH_HANDLE.load(std::sync::atomic::Ordering::Relaxed);
    if !handle.is_null() {
        unsafe {
            let mut written = 0;
            windows_sys::Win32::Storage::FileSystem::WriteFile(
                handle,
                message.as_ptr(),
                message.len() as u32,
                &mut written,
                std::ptr::null_mut(),
            );
            windows_sys::Win32::Storage::FileSystem::FlushFileBuffers(handle);
        }
    }
    0 // EXCEPTION_CONTINUE_SEARCH: preserve normal Windows crash handling.
}

#[cfg(test)]
mod tests {
    use super::*;
    fn archived(dir: &Path, at: u64, extra: usize) -> io::Result<PathBuf> {
        let p = dir.join(format!("nxmd-{at}-2147483647.log"));
        let mut f = File::create(&p)?;
        writeln!(
            f,
            "SystemTime {{ fixture }} INFO nxmd::diagnostics: nxmd test started"
        )?;
        if extra > 0 {
            f.set_len(extra as u64)?;
        }
        Ok(p)
    }
    #[test]
    fn retention_only_removes_recognized_inactive_logs() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let dir = tmp.path().join("logs");
        fs::create_dir(&dir)?;
        let now = SystemTime::now();
        let ms = now.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        let oversized = archived(&dir, ms - 2, 11 * 1024 * 1024)?;
        let old = archived(&dir, ms - 15 * 86400 * 1000, 0)?;
        let active = dir.join(format!("nxmd-1-{}.log", std::process::id()));
        fs::write(
            &active,
            "SystemTime { fixture } INFO nxmd::diagnostics: nxmd old started\n",
        )?;
        let unrelated = dir.join("nxmd-10-2147483647.log");
        fs::write(&unrelated, "user content")?;
        let draft = tmp.path().join("recovery-v1");
        fs::create_dir(&draft)?;
        fs::write(draft.join("draft.json"), "keep")?;
        for i in 10..26 {
            archived(&dir, ms - i, 0)?;
        }
        prune(&dir, now)?;
        assert!(!old.exists() && !oversized.exists() && active.exists());
        assert_eq!(fs::read(unrelated)?, b"user content");
        assert_eq!(fs::read(draft.join("draft.json"))?, b"keep");
        assert_eq!(fs::read_dir(&dir)?.count(), COUNT + 1);
        Ok(())
    }
    #[test]
    fn routine_records_cannot_exhaust_crash_reserve() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let log = Log::open(tmp.path())?;
        let message = "x".repeat(8192);
        for _ in 0..200 {
            log.append(&message);
        }
        assert!(log.crash.metadata()?.len() <= ROUTINE);
        let mut f = &log.crash;
        f.write_all(b"PANIC: fixture\n")?;
        let p = fs::read_dir(tmp.path().join("logs"))?
            .next()
            .unwrap()?
            .path();
        assert!(fs::read(p)?.ends_with(b"PANIC: fixture\n"));
        Ok(())
    }
    #[test]
    fn invalid_directory_leaves_other_data_untouched() -> io::Result<()> {
        let tmp = tempfile::tempdir()?;
        let p = tmp.path().join("logs");
        fs::write(&p, b"keep")?;
        assert!(Log::open(tmp.path()).is_err());
        assert_eq!(fs::read(p)?, b"keep");
        Ok(())
    }
}

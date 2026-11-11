//! Persistent diagnostics for GUI-subsystem builds; no note contents are logged.
use std::{io::Write, sync::OnceLock};
static LOG: OnceLock<std::path::PathBuf> = OnceLock::new();
struct Logger;
static LOGGER: Logger = Logger;
fn write(message: &str) {
    if let Some(path) = LOG.get() {
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            let _ = writeln!(f, "{:?} {message}", std::time::SystemTime::now());
            let _ = f.sync_data();
        }
    }
}
impl log::Log for Logger {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Info
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            write(&format!("{} {}: {}", r.level(), r.target(), r.args()));
        }
    }
    fn flush(&self) {}
}
pub fn init() {
    let dir = nxmd::config::Session::path().parent().unwrap().join("logs");
    if std::fs::create_dir_all(&dir).is_ok() {
        // Separate files prevent simultaneous instances from overwriting each other.
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let _ = LOG.set(dir.join(format!("nxmd-{stamp}-{}.log", std::process::id())));
    }
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(log::LevelFilter::Info);
    std::panic::set_hook(Box::new(|info| {
        write(&format!(
            "PANIC: {info}\n{}",
            std::backtrace::Backtrace::force_capture()
        ))
    }));
    #[cfg(windows)]
    unsafe {
        install_native_handler();
    }
    log::info!("nxmd {} started", env!("CARGO_PKG_VERSION"));
}
#[cfg(windows)]
static CRASH_HANDLE: std::sync::atomic::AtomicPtr<std::ffi::c_void> =
    std::sync::atomic::AtomicPtr::new(std::ptr::null_mut());
#[cfg(windows)]
unsafe fn install_native_handler() {
    use std::os::windows::io::IntoRawHandle;
    if let Some(path) = LOG.get() {
        if let Ok(file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            CRASH_HANDLE.store(file.into_raw_handle(), std::sync::atomic::Ordering::Relaxed);
        }
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

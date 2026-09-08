use std::io::{self, IsTerminal, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub struct Spinner {
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl Spinner {
    pub fn scan(path: &Path) -> Self {
        if !io::stderr().is_terminal() {
            return Self {
                running: Arc::new(AtomicBool::new(false)),
                handle: None,
            };
        }

        let running = Arc::new(AtomicBool::new(true));
        let thread_running = Arc::clone(&running);
        let path = path.display().to_string();
        write_frame("-", &path);

        let handle = thread::spawn(move || {
            let frames = ["-", "\\", "|", "/"];
            let mut frame = 1;

            while thread_running.load(Ordering::Relaxed) {
                write_frame(frames[frame], &path);
                frame = (frame + 1) % frames.len();
                thread::sleep(Duration::from_millis(100));
            }
        });

        Self {
            running,
            handle: Some(handle),
        }
    }
}

impl Drop for Spinner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
        clear_line();
    }
}

fn write_frame(frame: &str, path: &str) {
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r{frame} Scanning {path}");
    let _ = stderr.flush();
}

fn clear_line() {
    let mut stderr = io::stderr().lock();
    let _ = write!(stderr, "\r\x1b[2K");
    let _ = stderr.flush();
}

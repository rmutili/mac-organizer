//! Tracks running `claude -p` processes so a run can be cancelled, and so
//! quitting the app doesn't leave batches running (and using the person's
//! plan) in the background.

use std::collections::HashSet;
use std::process::Command;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

pub const CANCELLED: &str = "Cancelled.";

#[derive(Default)]
pub struct ProcessRegistry {
    pids: Mutex<HashSet<u32>>,
    cancelled: AtomicBool,
}

impl ProcessRegistry {
    /// Start a new run: clear any earlier cancellation.
    pub fn reset(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub fn register(&self, pid: u32) {
        self.lock().insert(pid);
    }

    pub fn unregister(&self, pid: u32) {
        self.lock().remove(&pid);
    }

    /// Mark the run cancelled and stop every running process.
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        self.kill_all();
    }

    /// Stop every running process (app exit). Returns how many were signalled.
    pub fn kill_all(&self) -> usize {
        let pids: Vec<u32> = self.lock().drain().collect();
        for pid in &pids {
            // SIGTERM lets the CLI exit cleanly; a pid that already exited just fails.
            let _ = Command::new("/bin/kill")
                .arg("-TERM")
                .arg(pid.to_string())
                .status();
        }
        pids.len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashSet<u32>> {
        // A poisoned set is still a valid set of pids.
        self.pids.lock().unwrap_or_else(|e| e.into_inner())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::process::ExitStatusExt;
    use std::process::Child;
    use std::time::{Duration, Instant};

    fn sleeper() -> Child {
        Command::new("/bin/sleep").arg("30").spawn().unwrap()
    }

    fn wait_briefly(child: &mut Child) -> Option<std::process::ExitStatus> {
        let started = Instant::now();
        while started.elapsed() < Duration::from_secs(5) {
            if let Some(status) = child.try_wait().unwrap() {
                return Some(status);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        None
    }

    #[test]
    fn cancel_sets_the_flag_and_stops_registered_processes() {
        let registry = ProcessRegistry::default();
        let mut child = sleeper();
        registry.register(child.id());

        registry.cancel();

        assert!(registry.is_cancelled());
        let status = wait_briefly(&mut child).expect("process should have stopped");
        assert!(status.signal().is_some());
    }

    #[test]
    fn unregistered_processes_are_left_alone() {
        let registry = ProcessRegistry::default();
        let mut child = sleeper();
        registry.register(child.id());
        registry.unregister(child.id());

        assert_eq!(registry.kill_all(), 0);
        assert!(child.try_wait().unwrap().is_none());
        child.kill().unwrap();
        child.wait().unwrap();
    }

    #[test]
    fn reset_clears_a_previous_cancellation() {
        let registry = ProcessRegistry::default();
        registry.cancel();

        registry.reset();

        assert!(!registry.is_cancelled());
    }
}

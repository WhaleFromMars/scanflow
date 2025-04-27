use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[cfg(feature = "progress_bar")]
use std::thread::{spawn, JoinHandle};
#[cfg(feature = "progress_bar")]
use std::time::Duration;

#[derive(Clone, Debug)]
pub enum PBarEvent {
    Start { max_length: u64, as_bytes: bool },
    Progress { current: u64 },
    Finished,
}

#[cfg(feature = "progress_bar")]
mod pbar_internals {
    use super::PBarEvent;
    use std::sync::{Mutex, OnceLock};

    pub type PBarEventCallback = Box<dyn Fn(PBarEvent) + Send + 'static>;

    pub static PBAR_CALLBACKS: OnceLock<Mutex<Vec<PBarEventCallback>>> = OnceLock::new();

    pub fn get_pbar_callbacks() -> &'static Mutex<Vec<PBarEventCallback>> {
        PBAR_CALLBACKS.get_or_init(|| Mutex::new(Vec::new()))
    }

    pub fn register_pbar_callback_impl<F>(callback: F)
    where
        F: Fn(PBarEvent) + Send + 'static,
    {
        if let Ok(mut callbacks) = get_pbar_callbacks().lock() {
            callbacks.push(Box::new(callback));
        } else {
            eprintln!("Failed to lock global callbacks mutex in register");
        }
    }
}

#[cfg(feature = "progress_bar")]
pub fn register_pbar_callback<F>(callback: F)
where
    F: Fn(PBarEvent) + Send + 'static,
{
    pbar_internals::register_pbar_callback_impl(callback);
}

#[cfg(not(feature = "progress_bar"))]
pub fn register_pbar_callback<F>(_callback: F)
where
    F: Fn(PBarEvent) + Send + 'static,
{
    // No-op when feature is disabled
}

/* Example Usage:
#[cfg(feature = "progress_bar")]
scanflow::pbar::register_pbar_callback(|event: PBarEvent| match event {
     PBarEvent::Start {
         max_length,
         as_bytes,
     } => {
         println!(
             "Progress started. Max length: {}, As bytes: {}",
             max_length, as_bytes
         );
     }
     PBarEvent::Progress { current } => {
         println!("Progress update: {}", current);
     }
     PBarEvent::Finished => {
         println!("Progress finished!");
     }
 });
 */

pub struct PBar {
    #[cfg(feature = "progress_bar")]
    handle: Option<JoinHandle<()>>,
    #[cfg(feature = "progress_bar")]
    count: Arc<AtomicU64>,
}

#[cfg(feature = "progress_bar")]
impl PBar {
    pub fn new(max_length: u64, as_bytes: bool) -> Self {
        let count = Arc::new(AtomicU64::new(0));
        let count2 = count.clone();

        Self {
            handle: Some(spawn(move || {
                let count = count2;
                let callbacks_lock = pbar_internals::get_pbar_callbacks();

                if let Ok(callbacks) = callbacks_lock.lock() {
                    for callback in callbacks.iter() {
                        callback(PBarEvent::Start {
                            max_length,
                            as_bytes,
                        });
                    }
                } else {
                    eprintln!("Failed to lock global callbacks mutex in PBar thread (start)");
                    return;
                }

                let timeout = Duration::from_millis(20);

                loop {
                    std::thread::sleep(timeout);
                    let loaded = count.load(Ordering::Acquire);

                    if loaded == !0 {
                        if let Ok(callbacks) = callbacks_lock.lock() {
                            for callback in callbacks.iter() {
                                callback(PBarEvent::Finished);
                            }
                        } else {
                            eprintln!(
                                "Failed to lock global callbacks mutex in PBar thread (finish)"
                            );
                        }
                        break;
                    }

                    if let Ok(callbacks) = callbacks_lock.lock() {
                        for callback in callbacks.iter() {
                            callback(PBarEvent::Progress { current: loaded });
                        }
                    } else {
                        eprintln!(
                            "Failed to lock global callbacks mutex in PBar thread (progress)"
                        );
                    }
                }
            })),
            count,
        }
    }

    pub fn add(&self, add: u64) {
        self.count.fetch_add(add, Ordering::Relaxed);
    }

    pub fn inc(&self) {
        self.add(1);
    }

    pub fn set(&self, value: u64) {
        self.count.store(value, Ordering::Relaxed);
    }

    pub fn finish(self) {}
}

#[cfg(feature = "progress_bar")]
impl Drop for PBar {
    fn drop(&mut self) {
        self.count.store(!0, Ordering::Release);
        self.handle.take().unwrap().join().unwrap();
    }
}

#[cfg(not(feature = "progress_bar"))]
impl PBar {
    pub fn new(_max_length: u64, _as_bytes: bool) -> Self {
        Self {}
    }

    pub fn add(&self, _add: u64) {}

    pub fn inc(&self) {}

    pub fn set(&self, _value: u64) {}

    pub fn finish(self) {}
}

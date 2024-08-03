//! genzero is a library that lets you get the latest value of a type.
//!
//! ## Why genzero?
//! There are many concurrency programs where one may have many reader
//! threads that want to get the latest value of a type that is being
//! updated by another thread (spmc). A naive approach would be using
//! a Mutex/RWLock on the shared piece of data. Locks are
//! unfortunately slow in many cases and specifically don't allow us
//! to maximize
//! throughput. [RCU](https://en.wikipedia.org/wiki/Read-copy-update)
//! is a technique initially developed in the linux kernel that allows
//! the pattern described above while not blocking the writers and
//! increasing throughput ([Fedor Pikus CppCon 2017 Talk on
//! RCU](https://www.youtube.com/watch?v=rxQ5K9lo034)). [crossbeam-epoch](https://docs.rs/crossbeam-epoch/latest/crossbeam_epoch/)
//! is an epoch based memory reclaimer that implements an RCU method
//! for Rust. genzero is an API to achieve this simple spmc latest
//! value pattern built on top of the crossbeam-epoch library.
//!
//! ## How to use genzero?
//! genzero's API is similar to a `crossbeam::channel` or a `std:sync::mpsc`
//!
//! ### Simple Example
//! ```rust
//! // Initialize first value to 0
//! let (mut tx, rx) = genzero::new::<u32>(0);
//!
//! tx.send(10);
//!
//! assert_eq!(rx.recv(), Some(10));
//! ```
//!
//! **NOTE: Once the [`Sender`] get's dropped all subsequent calls to `rx.recv()` will return `None`**

use crossbeam::epoch::{pin, Atomic, Owned, Shared};

use std::sync::atomic::Ordering;
use std::sync::Arc;

pub struct Sender<T: Clone> {
    inner_tx: Arc<Atomic<T>>,
}

#[derive(Clone)]
pub struct Receiver<T: Clone> {
    inner_rx: Arc<Atomic<T>>,
}

impl<T: Clone> Drop for Sender<T> {
    /// Will place a null pointer as the current value and then mark
    /// the last value as destroyable and then trigger an epoch flush
    /// to destroy the data and avoid memory leaks.
    /// All subsequent calls to `recv()` on the [`Receiver`] will
    /// return `None`
    fn drop(&mut self) {
        let guard = pin();
        let buf = self.inner_tx.swap(Shared::null(), Ordering::SeqCst, &guard);
        unsafe {
            guard.defer_destroy(buf);
        }
        guard.flush();
    }
}

/// Returns a [`Sender`] and [`Receiver`] of T and initializes the
/// value to `buf`.
pub fn new<T: Clone>(buf: T) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Atomic::new(buf));
    let tx = Sender {
        inner_tx: inner.clone(),
    };

    let rx = Receiver {
        inner_rx: inner.clone(),
    };

    (tx, rx)
}

impl<T: Clone> Sender<T> {
    /// Updates the epoch pointer to the value of buf and bumps the
    /// epoch generation.
    pub fn send(&mut self, buf: T) {
        let guard = pin();
        let old_buf = self
            .inner_tx
            .swap(Owned::new(buf), Ordering::Release, &guard);
        unsafe {
            guard.defer_destroy(old_buf);
        }
    }
}

impl<T: Clone> Receiver<T> {
    /// Get's the latest generation of the epoch pointer and returns a
    /// copy of the value. If the pointer is null then it will return
    /// `None`
    pub fn recv(&self) -> Option<T> {
        let guard = pin();
        let buf = self.inner_rx.load_consume(&guard);
        let inner_ref = unsafe { buf.as_ref() };

        match inner_ref {
            Some(b) => Some(b.clone()),
            None => None,
        }
    }
}

#[cfg(test)]

mod tests {
    use super::*;
    use rand::Rng;

    #[test]
    fn single_thread_reader_writer() {
        let (mut tx, rx) = new::<u32>(8);

        assert_eq!(rx.recv(), Some(8));

        tx.send(10);
        assert_eq!(rx.recv(), Some(10));
    }

    #[test]
    fn one_writer_one_reader_random_waits() {
        let (mut tx, rx) = new::<u32>(0);

        let t = std::thread::spawn(move || {
            let mut count = 0;
            let ten_millis = std::time::Duration::from_millis(10);

            for _n in 0..50 {
                count = count + 1;
                tx.send(count);
                std::thread::sleep(ten_millis);
            }
        });

        let mut rng = rand::thread_rng();

        loop {
            let v = rx.recv();
            if v == Some(50) || v == None {
                break;
            }
            let wait_time: u64 = rng.gen_range(0..50);
            let rand_millis = std::time::Duration::from_millis(wait_time);
            std::thread::sleep(rand_millis);
        }

        t.join().expect("writer didn't close cleanly");
    }
}

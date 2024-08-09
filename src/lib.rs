//! genzero is a library that lets you get the latest value of a type.
//!
//! ## Why genzero?
//! In concurrent programs, we often want many reader threads to get
//! the latest value from some writer thread (i.e., _single-producer, multi-consumer_, SPMC).
//! A naïve approach would protect shared data with a Mutex or RWLock,
//! but these can reduce throuhgput due to contention and cache line ping-pong.
//!
//! A faster SPMC approach is [RCU](https://en.wikipedia.org/wiki/Read-copy-update).[^1]
//! Initially developed in the Linux kernel, it lets
//! readers locklessly read the latest version *and continue referencing it*
//! without blocking the writer from publishing new versions in the meantime.
//!
//! genzero provides a simple and safe API on top of
//! [crossbeam-epoch](https://docs.rs/crossbeam-epoch/latest/crossbeam_epoch/),
//! an RCU implementation for Rust.
//!
//! ## How do I use use genzero?
//! genzero's API is similar to a `crossbeam::channel` or a `std:sync::mpsc`:
//!
//! ```rust
//! // Start with nothing; you can also provide an initial value with genzero::new()
//! let (mut tx, rx) = genzero::empty();
//! assert_eq!(rx.recv(), None);
//!
//! tx.send(10);
//!
//! assert_eq!(rx.recv(), Some(10));
//!
//! // Once the sender is dropped, the receiver gets None.
//! drop(tx);
//! assert_eq!(rx.recv(), None);
//! ```
//!
//! Due to the magic of ~~garbage collection~~ deferred reclamation,
//! you can also borrow the latest value and hold that reference as long as you want,
//! _completely independent of the sender or receiver's lifetime_.
//!
//! ```rust
//! let (mut tx, rx) = genzero::new(42);
//!
//! let b = rx.borrow().expect("borrow was None");
//! assert_eq!(*b, 42);
//!
//! // Totally safe:
//! drop(tx);
//! assert_eq!(*b, 42);
//!
//! // ...even though new reads give us None:
//! assert_eq!(rx.recv(), None);
//!
//! // Still!
//! drop(rx);
//! assert_eq!(*b, 42);
//! ```
//! [^1]: See the [Linux kernel docs](https://www.kernel.org/doc/html/latest/RCU/whatisRCU.html)
//!       or Fedor Pikus's [CppCon 2017 presentation](https://www.youtube.com/watch?v=rxQ5K9lo034)
//!       to learn more.

use crossbeam::epoch::{pin, Atomic, Guard, Owned, Shared};

use std::sync::atomic::Ordering;
use std::sync::Arc;

/// Updates receivers with the newest value.
pub struct Sender<T> {
    inner_tx: Arc<Atomic<T>>,
}

/// Clones or borrows the newest value from the sender.
#[derive(Clone)]
pub struct Receiver<T> {
    inner_rx: Arc<Atomic<T>>,
}

impl<T> Drop for Sender<T> {
    fn drop(&mut self) {
        // Atomically swap the value inside with null.
        let guard = pin();
        let v = self.inner_tx.swap(Shared::null(), Ordering::SeqCst, &guard);
        // If we had a value in there, mark it for deletion
        // as soon as all readers are done with it.
        if !v.is_null() {
            unsafe {
                guard.defer_destroy(v);
            }
        }
        // Optional, but useful since we probably don't drop senders until it's time to go:
        // Flush the thread-local cache of deferred deletes.
        guard.flush();
    }
}

/// Build a new [`Sender`] and [`Receiver`] pair, initialized to `v`.
pub fn new<T>(v: T) -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Atomic::new(v));
    let tx = Sender {
        inner_tx: inner.clone(),
    };
    let rx = Receiver { inner_rx: inner };
    (tx, rx)
}

/// Build a new [`Sender`] and [`Receiver`] pair that starts empty.
///
/// Useful for when there's no sane default value.
pub fn empty<T>() -> (Sender<T>, Receiver<T>) {
    let inner = Arc::new(Atomic::null());
    let tx = Sender {
        inner_tx: inner.clone(),
    };
    let rx = Receiver { inner_rx: inner };
    (tx, rx)
}

impl<T> Sender<T> {
    /// Publish a new value to the matching [`Receiver`]s
    pub fn send(&mut self, v: T) {
        let guard = pin();
        let prev = self.inner_tx.swap(Owned::new(v), Ordering::Release, &guard);
        if !prev.is_null() {
            unsafe {
                guard.defer_destroy(prev);
            }
        }
    }
}

/// A reference to the current value, as of the time it was returend from [`Receiver::borrow()`].
///
/// Can outlive both sender and receiver!
pub struct Borrow<T> {
    _guard: Guard,
    // Ideally this would be a Shared,
    // but that depends on the lifetime of the guard, and Rust doesn't like self-reference.
    // SAFETY: the pointer is valid so long as we have the guard (i.e., epoch) we loaded it from.
    shared: *const T,
}

impl<T> std::ops::Deref for Borrow<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // SAFETY: the pointer is valid so long as we hold the guard.
        // Invariant: We don't make a Borrow pointing to null.
        unsafe { self.shared.as_ref().unwrap() }
    }
}

impl<T> Receiver<T> {
    /// Borrows the current value for as long as you want.
    ///
    /// Just because you *can* hold onto this borrow indefinitely dones't mean you should.
    /// The [`Sender`] is presumably publishing new versions, making it increasingly stale!
    pub fn borrow(&self) -> Option<Borrow<T>> {
        let guard = pin();
        let shared = self.inner_rx.load_consume(&guard).as_raw(); // This one's for Paul.
        if shared.is_null() {
            None
        } else {
            Some(Borrow {
                _guard: guard,
                shared,
            })
        }
    }
}

impl<T: Clone> Receiver<T> {
    /// Clones the current value.
    ///
    /// If cloning isn't cheap (or possible!) consider [`borrow()`](Receiver::borrow)
    pub fn recv(&self) -> Option<T> {
        let guard = pin();
        let v = self.inner_rx.load_consume(&guard); // memory_order_consume lives!
        let inner_ref = unsafe { v.as_ref() };
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
    fn empty_after_drop() {
        let (tx, rx) = new::<u32>(42);
        assert_eq!(rx.recv(), Some(42));
        drop(tx);
        assert_eq!(rx.recv(), None);
    }

    #[test]
    fn borrow_after_drop() {
        let (tx, rx) = new::<u32>(42);
        let b = match rx.borrow() {
            Some(s) => s,
            None => panic!("Empty borrow after init"),
        };

        drop(tx);
        assert_eq!(rx.recv(), None);

        // So long as we hold the pin, nothing bad happens.
        // Once the pin is pulled, Mr. Grenade is not your friend.
        assert_eq!(*b, 42);
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

    #[test]
    fn one_writer_one_reader_borrows() {
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
            match rx.borrow() {
                Some(b) if *b == 50 => break,
                None => break,
                _ => (),
            }
            let wait_time: u64 = rng.gen_range(0..50);
            let rand_millis = std::time::Duration::from_millis(wait_time);
            std::thread::sleep(rand_millis);
        }

        t.join().expect("writer didn't close cleanly");
    }
}

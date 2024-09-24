# genzero

genzero is a library that lets you get the latest value of a type.

## Why genzero?

In concurrent programs, we often want many reader threads to get
the latest value from some writer thread (i.e., _single-producer, multi-consumer_, SPMC).
A naïve approach would protect shared data with a Mutex or RWLock,
but these can reduce throuhgput due to contention and cache line ping-pong.

A faster SPMC approach is [RCU](https://en.wikipedia.org/wiki/Read-copy-update).[^1]
Initially developed in the Linux kernel, it lets
readers locklessly read the latest version *and continue referencing it*
without blocking the writer from publishing new versions in the meantime.

genzero provides a simple and safe API on top of
[crossbeam-epoch](https://docs.rs/crossbeam-epoch/latest/crossbeam_epoch/),
an RCU implementation for Rust.

## How do I use use genzero?
genzero's API is similar to a `crossbeam::channel` or a `std:sync::mpsc`:

```rust
// Start with nothing; you can also provide an initial value with genzero::new()
let (mut tx, rx) = genzero::empty();
assert_eq!(rx.recv(), None);

tx.send(10);

assert_eq!(rx.recv(), Some(10));

// Once the sender is dropped, the receiver gets None.
drop(tx);
assert_eq!(rx.recv(), None);
```

Due to the magic of ~~garbage collection~~ deferred reclamation,
you can also borrow the latest value and hold that reference as long as you want,
_completely independent of the sender or receiver's lifetime_.

```rust
let (mut tx, rx) = genzero::new(42);

let b = rx.borrow().expect("borrow was None");
assert_eq!(*b, 42);

// Totally safe:
drop(tx);
assert_eq!(*b, 42);

// ...even though new reads give us None:
assert_eq!(rx.recv(), None);

// Still!
drop(rx);
assert_eq!(*b, 42);
```

[^1]: See the [Linux kernel docs](https://www.kernel.org/doc/html/latest/RCU/whatisRCU.html)
      or Fedor Pikus's [CppCon 2017 presentation](https://www.youtube.com/watch?v=rxQ5K9lo034)
      to learn more.

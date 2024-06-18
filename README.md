# genzero

genzero is a library that lets you get the latest value of a type.

## Why genzero?
There are many concurrency programs where one may have many reader
threads that want to get the latest value of a type that is being
updated by another thread (spmc). A naive approach would be using
a Mutex/RWLock on the shared piece of data. Locks are
unfortunately slow in many cases and specifically don't allow us
to maximize
throughput. [RCU](https://en.wikipedia.org/wiki/Read-copy-update)
is a technique initially developed in the linux kernel that allows
the pattern described above while not blocking the writers and
increasing throughput ([Fedor Pikus CppCon 2017 Talk on
RCU](https://www.youtube.com/watch?v=rxQ5K9lo034)). [crossbeam-epoch](https://docs.rs/crossbeam-epoch/latest/crossbeam_epoch/)
is an epoch based memory reclaimer that implements an RCU method
for Rust. genzero is an API to achieve this simple spmc latest
value pattern built on top of the crossbeam-epoch library.

## How to use genzero?
genzero's API is similar to a `crossbeam::channel` or a `std:sync::mpsc`

### Simple Example
```rust
// Initialize first value to 0
let (mut tx, rx) = genzero::new<u32>(0);

tx.send(10);

assert_eq!(rx.recv(), Some(10));
```

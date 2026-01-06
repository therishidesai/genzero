pub fn main() {
    let lock = std::sync::Arc::new(std::sync::RwLock::new(0));

    let reader_lock = lock.clone();

    let writer = std::thread::spawn(move || {
        let mut count = 0;
        let ten_millis = std::time::Duration::from_millis(10);
        let messages = 1000;
        let mut nanos = 0u128;
        for _n in 0..messages {
            count = count + 1;
            let now = std::time::Instant::now();
            let mut v = lock.write().unwrap();
            *v = count;
            nanos += now.elapsed().as_nanos();
            std::thread::sleep(ten_millis);
        }

        eprintln!("Average send time in nanos: {}", (nanos/messages) as f32);
    });

    let _reader = std::thread::spawn(move || {
        let hundred_millis = std::time::Duration::from_millis(100);
        loop {
            let v = reader_lock.read().unwrap();
            if *v == 1000 {
                break;
            }

            std::thread::sleep(hundred_millis);
        }
    });

    writer.join().expect("writer didn't close cleanly");
}

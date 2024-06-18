use rand::Rng;

pub fn main() {
    let (mut tx, rx) = genzero::new::<u32>(0);

    let t = std::thread::spawn(move || {
        let mut count = 0;
        let ten_millis = std::time::Duration::from_millis(10);

        for _n in 0..50 {
            count = count + 1;
            tx.send(count);
            std::thread::sleep(ten_millis);
        }

    });

    let rx1 = rx.clone();
    let tr = std::thread::spawn(move || {
        let mut rng = rand::thread_rng();

        loop {
            let v = rx1.recv();
            println!("reader 1: {:?}", v);
            if v == Some(50) || v == None {
                break;
            }
            let wait_time: u64 = rng.gen_range(0..50);
            let rand_millis = std::time::Duration::from_millis(wait_time);
            std::thread::sleep(rand_millis);
        }
       
    });

    let mut rng = rand::thread_rng();

    loop {
        let v = rx.recv();
        println!("reader 0: {:?}", v);
        if v == Some(50) || v == None {
            break;
        }
        let wait_time: u64 = rng.gen_range(0..50);
        let rand_millis = std::time::Duration::from_millis(wait_time);
        std::thread::sleep(rand_millis);
    }

    t.join().expect("writer didn't close cleanly");
    tr.join().expect("second reader didn't close cleanly");
}


fn main() {
    const OPS: usize = 1024 * 1024 * 16;

    let mut t = 0;

    for thread_count in 8..=24 {
        // println!("{:=>100}", "");
        // println!("thread count = {thread_count}");
        let barrier = std::sync::Barrier::new(thread_count);

        let start = std::time::Instant::now();
        std::thread::scope(|s| {
            let barrier = &barrier;
            // let start = std::time::Instant::now();

            let callback = move |_, mut buf: String| {
                barrier.wait();
                buf.reserve(64);
                let len = 1024;
                buf.truncate(len);
                assert!(buf.len() == len);
                // let len = buf.len();

                istr::new(&buf);

                for _ in 0..OPS {
                    // buf.truncate(len);
                    // buf.push_str(op);
                    istr::new(&buf);
                    // istr::get(&buf).unwrap();
                    // istr::new_skip_local(&buf);
                }

                // let time = start.elapsed();

                // println!("{t} => {:.3} ns", time.as_secs_f64() / OPS as f64 * 1e9);
                // dbg!((start.elapsed(), istr::local_cache_size()));
            };
            t += 1;
            for _ in 0..thread_count {
                let name = "thread info {t}".repeat(1000);
                if thread_count == 1 {
                    callback(0, name)
                } else {
                    std::thread::Builder::new()
                        .spawn_scoped(s, move || callback(t, name))
                        .unwrap();
                }
            }
        });

        let time = start.elapsed();

        // println!("{time:?}");
        println!(
            "{thread_count},{:.3}",
            time.as_secs_f64() / OPS as f64 * 1e9
        );
        // println!("{:?}", istr::size());
    }
}

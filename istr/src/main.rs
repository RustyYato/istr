fn main() {
    let path = std::env::args_os().nth(1).unwrap();
    let text = std::fs::read(path).unwrap();
    // let text = text.repeat(64);

    let start = std::time::Instant::now();

    std::thread::scope(|s| {
        for i in 0..24 {
            s.spawn(|| {
                run::<true>(&text, |word| {
                    istr::IBytes::new(word);
                    // istr::IBytes::new_skip_local(word);
                    // ustr::ustr(unsafe { core::str::from_utf8_unchecked(word) });
                });
            });
        }
    });

    dbg!(start.elapsed());

    let len = istr::items().map(istr::IBytes::len).sum::<usize>();

    println!("{len}")
}

// fn run_all<const INCLUDE_NON_WORDS: bool, const SKIP_LOCAL: bool>(s: &[u8]) {
//     let start = std::time::Instant::now();
//     run::<false>(s, |bytes| {
//         if SKIP_LOCAL {
//             istr::IBytes::new_skip_local(bytes);
//         } else {
//             istr::IBytes::new(bytes);
//         }
//     });
//     println!("{:?}", start.elapsed());

//     let start = std::time::Instant::now();
//     run::<false>(s, |bytes| {
//         ustr::ustr(unsafe { core::str::from_utf8_unchecked(bytes) });
//     });
//     println!("{:?}", start.elapsed());
// }

fn run<const INCLUDE_NON_WORDS: bool>(mut s: &[u8], f: impl Fn(&[u8])) {
    loop {
        let Some(index) = s
            .iter()
            .position(|&x| !matches!(x, b'a'..=b'z' | b'A'..=b'Z')) else {

            f(s);
            break;
        };

        let (text, next) = s.split_at(index);

        f(text);

        let Some(index) = next
            .iter()
            .position(|&x| matches!(x, b'a'..=b'z' | b'A'..=b'Z')) else {
            break;
        };

        let (text, next) = next.split_at(index);

        if INCLUDE_NON_WORDS {
            f(text);
        }

        s = next
    }
}

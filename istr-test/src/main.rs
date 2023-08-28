use std::{num::NonZeroUsize, path::PathBuf};

#[derive(clap::Parser)]
pub struct Args {
    path: PathBuf,
    #[clap(long)]
    threads: Option<NonZeroUsize>,
}

fn main() {
    let args: Args = clap::Parser::parse();
    let text = std::fs::read(args.path).unwrap();
    // let text = text.repeat(64);

    let start = std::time::Instant::now();

    let threads = args
        .threads
        .or_else(|| std::thread::available_parallelism().ok())
        .unwrap_or(NonZeroUsize::new(1).unwrap())
        .get();

    println!("Running test on {threads} threads");

    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                run::<true, _>(&text, |word| {
                    ustr::ustr(unsafe { core::str::from_utf8_unchecked(word) })
                });
            });
        }
    });

    dbg!(start.elapsed());

    let start = std::time::Instant::now();

    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                run::<true, _>(&text, istr::IBytes::new_skip_local);
            });
        }
    });

    dbg!(start.elapsed());

    #[cfg(ISTR_GLOBAL_CACHE_CLEAR)]
    istr::clear_global_cache();

    let start = std::time::Instant::now();

    std::thread::scope(|s| {
        for _ in 0..threads {
            s.spawn(|| {
                run::<true, _>(&text, istr::IBytes::new);
            });
        }
    });

    dbg!(start.elapsed());

    dbg!(ustr::num_entries());
    dbg!(istr::size());
}

fn run<const INCLUDE_NON_WORDS: bool, R>(mut s: &[u8], f: impl Fn(&[u8]) -> R) {
    loop {
        let Some(index) = s
            .iter()
            .position(|&x| !x.is_ascii_alphabetic()) else {

            f(s);
            break;
        };

        let (text, next) = s.split_at(index);

        f(text);

        let Some(index) = next
            .iter()
            .position(|&x| x.is_ascii_alphabetic()) else {
            break;
        };

        let (text, next) = next.split_at(index);

        if INCLUDE_NON_WORDS {
            f(text);
        }

        s = next
    }
}

run:
    RUSTFLAGS='--cfg ISTR_GLOBAL_CACHE_CLEAR' cargo run -r -- fixtures/long_text.txt --threads 1
bench:
    RUSTFLAGS='--cfg ISTR_GLOBAL_CACHE_CLEAR' cargo flamegraph --features ustr -- fixtures/long_text.txt
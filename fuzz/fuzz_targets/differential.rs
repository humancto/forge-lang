#![no_main]

libfuzzer_sys::fuzz_target!(|data: &[u8]| {
    forge_lang_fuzz::harness::check_differential(data);
});

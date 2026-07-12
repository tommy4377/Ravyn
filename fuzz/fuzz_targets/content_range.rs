#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = std::str::from_utf8(data) {
        let _ = ravyn::download::http::validate_resume_range(Some(value), 128, Some(4096));
    }
});

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = ravyn::native_protocol::decode_framed_json(data, 1_048_576);
});

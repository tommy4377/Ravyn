#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    ravyn::adapters::torrent::normalize_rqbit_payload_for_fuzzing(data);
});

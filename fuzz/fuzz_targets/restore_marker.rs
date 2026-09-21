#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let _ = serde_json::from_slice::<ravyn::storage::recovery::PendingRestore>(data);
    let _ = serde_json::from_slice::<ravyn::storage::recovery::RestoreResultRecord>(data);
});

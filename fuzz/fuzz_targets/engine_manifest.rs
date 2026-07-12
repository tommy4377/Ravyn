#![no_main]

use libfuzzer_sys::fuzz_target;
use ravyn::services::engines::SignedEngineManifest;

fuzz_target!(|data: &[u8]| {
    if let Ok(manifest) = serde_json::from_slice::<SignedEngineManifest>(data) {
        let _ = manifest.verify(&[0; 32]);
    }
});

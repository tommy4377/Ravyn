#![no_main]

use clap::Parser;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = std::str::from_utf8(data) {
        let config = ravyn::config::Config::parse_from(["ravyn"]);
        let _ = ravyn::services::security::validate_network_source(&config, value);
        let _ = ravyn::services::filename::from_url(value);
    }
});

#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(value) = std::str::from_utf8(data) {
        if let Ok(expression) = ravyn::services::cron::CronExpression::parse(value) {
            let _ = expression.next_after(chrono::Utc::now());
        }
    }
});

use std::{ffi::CString, sync::OnceLock};

fn map_level(l: log::Level) -> libc::c_int {
    match l {
        log::Level::Error => 0, // FATAL
        log::Level::Warn => 1,  // INFO (closest tier)
        log::Level::Info => 1,  // INFO
        log::Level::Debug => 2, // DEBUG
        log::Level::Trace => 2, // map TRACE -> DEBUG
    }
}

fn sanitize_to_c(s: &str) -> CString {
    // ensure no interior NULs
    CString::new(s.replace('\0', "␀")).expect("CString construction failed")
}

struct DiceLogger {
    max: log::LevelFilter,
}

impl log::Log for DiceLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= self.max
    }

    fn log(&self, record: &log::Record) {
        if !self.enabled(record.metadata()) {
            return;
        }
        let c = sanitize_to_c(&record.args().to_string());
        unsafe { crate::raw::dice_log_write(map_level(record.level()), c.as_ptr()) };
    }

    fn flush(&self) {}
}

static LOGGER: OnceLock<DiceLogger> = OnceLock::new();

pub fn init(max_level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    let logger = LOGGER.get_or_init(|| DiceLogger { max: max_level });
    log::set_logger(logger)?;
    log::set_max_level(max_level);
    Ok(())
}

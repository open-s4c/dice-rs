//! Logging integration for dice.
//!
//! This module provides a [`log`] crate backend that routes Rust log messages
//! through dice's logging infrastructure. This ensures that log output from
//! Rust code appears alongside C-side dice logs with consistent formatting.
//!
//! # Usage
//!
//! Call [`init`] early in your program (typically via [`init_dice_state!`](crate::init_dice_state)):
//!
//! ```ignore
//! dice_rs::log::init(log::LevelFilter::Debug)?;
//! log::info!("Hello from Rust!");
//! ```
//!
//! # Level Mapping
//!
//! Rust log levels are mapped to dice's C logging levels:
//!
//! | Rust Level | Dice Level |
//! |------------|------------|
//! | `Error`    | 0 (FATAL)  |
//! | `Warn`     | 1 (INFO)   |
//! | `Info`     | 1 (INFO)   |
//! | `Debug`    | 2 (DEBUG)  |
//! | `Trace`    | 2 (DEBUG)  |

use std::{ffi::CString, sync::OnceLock};

/// Map Rust log levels to dice C log levels.
fn map_level(l: log::Level) -> libc::c_int {
    match l {
        log::Level::Error => 0, // FATAL
        log::Level::Warn => 1,  // INFO (closest tier)
        log::Level::Info => 1,  // INFO
        log::Level::Debug => 2, // DEBUG
        log::Level::Trace => 2, // map TRACE -> DEBUG
    }
}

/// Convert a Rust string to a C string, replacing interior NULs.
///
/// Interior NUL bytes would truncate the C string, so we replace them
/// with a visible placeholder character.
fn sanitize_to_c(s: &str) -> CString {
    CString::new(s.replace('\0', "␀")).expect("CString construction failed after NUL replacement")
}

/// Log backend that forwards to dice's C logging infrastructure.
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
        /* SAFETY: raw::dice_log_write preconditions (from raw.rs):
         * - level valid (0=FATAL, 1=INFO, 2=DEBUG): map_level returns only 0, 1, or 2
         * - msg valid null-terminated C string: CString guarantees null-termination
         * - msg valid for call duration: c lives until end of this statement
         */
        unsafe { crate::raw::dice_log_write(map_level(record.level()), c.as_ptr()) };
    }

    fn flush(&self) {}
}

/// Global logger instance, initialized once.
static LOGGER: OnceLock<DiceLogger> = OnceLock::new();

/// Initialize the dice logging backend.
///
/// This registers a [`log`] backend that forwards messages to dice's C logging
/// infrastructure. Should be called once at program startup.
///
/// # Arguments
///
/// * `max_level` - Maximum log level to capture. Messages above this level are ignored.
///
/// # Errors
///
/// Returns an error if a logger has already been set (by this or another logging backend).
///
/// # Example
///
/// ```ignore
/// dice_rs::log::init(log::LevelFilter::Debug)?;
/// log::info!("Logging initialized");
/// ```
pub fn init(max_level: log::LevelFilter) -> Result<(), log::SetLoggerError> {
    let logger = LOGGER.get_or_init(|| DiceLogger { max: max_level });
    log::set_logger(logger)?;
    log::set_max_level(max_level);
    Ok(())
}

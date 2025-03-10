use env_logger::WriteStyle;
use log::LevelFilter;
use owo_colors::{AnsiColors, OwoColorize};
use std::io::Write;

pub const LOG_LEVEL_COLORS: [AnsiColors; 5] = [
    AnsiColors::BrightRed,
    AnsiColors::Yellow,
    AnsiColors::BrightBlue,
    AnsiColors::Green,
    AnsiColors::Default,
];

#[cfg(debug_assertions)]
pub const LOG_LEVEL: LevelFilter = LevelFilter::Debug;
#[cfg(not(debug_assertions))]
pub const LOG_LEVEL: LevelFilter = LevelFilter::Info;

/// Initialize the logger with the default format
#[cold]
pub(crate) fn setup_logger() {
    env_logger::builder()
        .parse_default_env()
        .filter_level(LOG_LEVEL)
        .write_style(WriteStyle::Auto)
        .format(|formatter, record| {
            let level = record.level();
            // levels start at ordinal 1, so we need to shift them down by 1
            let color = LOG_LEVEL_COLORS[(level as usize) - 1];

            writeln!(
                formatter,
                "[{level}]: {message}",
                level = level.color(color),
                message = record.args()
            )
        })
        .init();
}

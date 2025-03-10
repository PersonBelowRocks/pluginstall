//! Utilities for controlling the output of the CLI app.

use std::io::{Stdout, Write};

/// A helper struct for controlling the output from the CLI. Data can be "written" to the output manager, and it will
/// choose the appropriate format to output it in.
pub struct OutputManager {
    json: bool,
    stdout: Stdout,
}

/// Trait implemented by data that can be outputted/displayed from the CLI app. Implementors of this trait should be "output"
/// types that contain relevant data to be outputted.
///
/// Subcommands should probably each have their own "output" type that implements this trait. This is because each
/// subcommand can be thought of as being associated with a "UI panel" that displays the output of the command in a
/// nice-to-read format.
///
/// This is not a [`std::fmt::Debug`] or [`std::fmt::Display`] analogue, since those types are meant to be implemented by
/// *any* data that could be converted to text. This type is specifically for entire "panels" of data produced by commands/subcommands.
pub trait DataDisplay {
    fn write_json(&self, w: &mut impl Write) -> Result<(), std::io::Error>;

    fn write_hr(&self, w: &mut impl Write) -> Result<(), std::io::Error>;
}

impl OutputManager {
    /// Create a new output manager.
    ///
    /// If the [`json`] parameter is true then this output manager will write data in JSON format instead of a human-readable format.
    #[inline]
    pub fn new(json: bool) -> Self {
        Self {
            json,
            stdout: std::io::stdout(),
        }
    }

    /// Write the data display type to `stdout`. This method locks `stdout`.
    #[inline]
    pub fn display<T: DataDisplay>(&self, data: T) -> Result<(), std::io::Error> {
        let mut lock = self.stdout.lock();

        if self.json {
            data.write_json(&mut lock)?;
        } else {
            data.write_hr(&mut lock)?;
        }

        // flush it to make sure everything is written!
        lock.flush()?;

        Ok(())
    }
}

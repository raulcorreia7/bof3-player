// SPDX-License-Identifier: MIT
//! The mappings from the library crates' errors to a status.

pub(crate) use psx_capi::{Fail, fail, guard, map_formats, psx_status};

/// The status for one of the audio library's errors.
pub(crate) fn map_audio(error: bof3player::Error) -> Fail {
    use bof3player::Error;
    match error {
        Error::Io(message) => Fail::new(psx_status::Io, message),
        Error::Format(message) => Fail::new(psx_status::Format, message),
        Error::Invalid(message) => Fail::new(psx_status::InvalidArgument, message),
        Error::Unsupported(message) => Fail::new(psx_status::Unsupported, message),
        Error::State(message) => Fail::new(psx_status::Usage, message),
        other => Fail::new(psx_status::Format, other.to_string()),
    }
}

/// The status for a disc-image error.
pub(crate) fn map_cd(error: psx_cd::Error) -> Fail {
    use psx_cd::Error;
    match error {
        Error::Io(error) => Fail::new(psx_status::Io, error.to_string()),
        Error::NoImage(message) => Fail::new(psx_status::Io, message),
        Error::InvalidImage(message) => Fail::new(psx_status::Format, message),
        Error::InvalidImageDetail(message) => Fail::new(psx_status::Format, message),
        Error::InvalidInput(message) => Fail::new(psx_status::Format, message),
        Error::Patch(message) => Fail::new(psx_status::Format, message),
    }
}

/// The status for a machine error.
pub(crate) fn map_machine(error: psx_machine::Error) -> Fail {
    use psx_machine::Error;
    match error {
        Error::Disc(error) => map_cd(error),
        Error::Boot(message) => Fail::new(psx_status::Format, message),
        other => Fail::new(psx_status::Format, other.to_string()),
    }
}

/// The failure for a poisoned mutex: a previous call panicked while it held
/// the handle.
pub(crate) fn poisoned() -> Fail {
    Fail::new(psx_status::Panic, "the handle panicked earlier; reopen it")
}

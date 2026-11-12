// SPDX-License-Identifier: MIT
//! Status mapping, the thread-local last error and the panic boundary.

use core::ffi::c_char;
use std::any::Any;
use std::cell::RefCell;
use std::ffi::CString;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

use crate::bof3player_status;

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// A failed call: the status to return and the message for the last-error slot.
pub(crate) struct Fail {
    pub(crate) status: bof3player_status,
    pub(crate) message: String,
}

impl Fail {
    pub(crate) fn new(status: bof3player_status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }
}

/// Set the thread-local error and return `status`, for checks that run before
/// the panic boundary.
pub(crate) fn fail(status: bof3player_status, message: impl AsRef<str>) -> bof3player_status {
    set_error(message);
    status
}

/// Store the message of the last failing call on this thread.
pub(crate) fn set_error(message: impl AsRef<str>) {
    let text = message.as_ref().replace('\0', " ");
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(text).ok();
    });
}

/// Forget the last error, at the start of every fallible call.
pub(crate) fn clear_error() {
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

/// The NUL-terminated last error, or NULL.
pub(crate) fn last_error() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ref().map_or(ptr::null(), |text| text.as_ptr()))
}

/// Clear the error, run `body`, and turn a failure or a panic into a status.
///
/// A panic never crosses the C boundary: it is caught here, its message is
/// stored, and the call reports [`bof3player_status::Panic`].
pub(crate) fn guard(body: impl FnOnce() -> Result<(), Fail>) -> bof3player_status {
    clear_error();
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => bof3player_status::Ok,
        Ok(Err(error)) => {
            set_error(error.message);
            error.status
        }
        Err(payload) => {
            set_error(format!("panic: {}", panic_text(payload)));
            bof3player_status::Panic
        }
    }
}

fn panic_text(payload: Box<dyn Any + Send>) -> String {
    if let Some(text) = payload.downcast_ref::<&str>() {
        (*text).to_string()
    } else if let Some(text) = payload.downcast_ref::<String>() {
        text.clone()
    } else {
        String::from("panic")
    }
}

/// The status for one of the audio library's errors.
pub(crate) fn map_audio(error: bof3player::Error) -> Fail {
    use bof3player::Error;
    match error {
        Error::Io(message) => Fail::new(bof3player_status::Io, message),
        Error::Format(message) => Fail::new(bof3player_status::Format, message),
        Error::Invalid(message) => Fail::new(bof3player_status::InvalidArgument, message),
        Error::Unsupported(message) => Fail::new(bof3player_status::Unsupported, message),
        Error::State(message) => Fail::new(bof3player_status::Usage, message),
        other => Fail::new(bof3player_status::Format, other.to_string()),
    }
}

/// The status for an EMI archive error.
pub(crate) fn map_emi(error: emi::Error) -> Fail {
    use emi::Error;
    match error {
        Error::Io(error) => Fail::new(bof3player_status::Io, error.to_string()),
        Error::Format(error) => map_formats(error),
        Error::InvalidArchive(message) => Fail::new(bof3player_status::Format, format!("invalid EMI archive: {message}")),
        Error::EntryOutOfRange { index, count } => Fail::new(bof3player_status::InvalidIndex, format!("entry index {index} is out of range (the archive has {count} entries)")),
        Error::CueTableSize(bytes) => Fail::new(bof3player_status::Format, format!("cue table is {bytes} bytes")),
        Error::CueTableRecords(records) => Fail::new(bof3player_status::Format, format!("cue table holds {records} records")),
        Error::CueRecord { record, reason } => Fail::new(bof3player_status::Format, format!("cue record {record}: {reason}")),
    }
}

/// The status for a disc-image error.
pub(crate) fn map_cd(error: psx_cd::Error) -> Fail {
    use psx_cd::Error;
    match error {
        Error::Io(error) => Fail::new(bof3player_status::Io, error.to_string()),
        Error::NoImage(message) => Fail::new(bof3player_status::Io, message),
        Error::InvalidImage(message) => Fail::new(bof3player_status::Format, message),
        Error::InvalidImageDetail(message) => Fail::new(bof3player_status::Format, message),
        Error::InvalidInput(message) => Fail::new(bof3player_status::Format, message),
        Error::Patch(message) => Fail::new(bof3player_status::Format, message),
    }
}

/// The status for a machine error.
pub(crate) fn map_machine(error: psx_machine::Error) -> Fail {
    use psx_machine::Error;
    match error {
        Error::Disc(error) => map_cd(error),
        Error::Boot(message) => Fail::new(bof3player_status::Format, message),
        other => Fail::new(bof3player_status::Format, other.to_string()),
    }
}

/// The status for one of the format library's errors.
pub(crate) fn map_formats(error: psx_formats::Error) -> Fail {
    let status = match &error {
        psx_formats::Error::VagOutOfRange { .. } | psx_formats::Error::SepSequenceOutOfRange { .. } => bof3player_status::InvalidIndex,
        psx_formats::Error::WaveUnsupported(_) => bof3player_status::Unsupported,
        _ => bof3player_status::Format,
    };
    Fail::new(status, error.to_string())
}

/// The failure for a poisoned mutex: a previous call panicked while it held
/// the handle.
pub(crate) fn poisoned() -> Fail {
    Fail::new(bof3player_status::Panic, "the handle panicked earlier; reopen it")
}

// SPDX-License-Identifier: MIT
//! The status codes, the thread-local last error and the panic boundary.

use core::ffi::c_char;
use std::any::Any;
use std::cell::RefCell;
use std::ffi::CString;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;

/// Status returned by every fallible C function.
#[repr(C)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum psx_status {
    /// The call succeeded.
    Ok = 0,
    /// A pointer, length or value the caller supplied is invalid.
    InvalidArgument = 1,
    /// An index is past the last item or entry.
    InvalidIndex = 2,
    /// The input is valid but uses a feature this library does not implement.
    Unsupported = 3,
    /// A file or stream could not be read or written.
    Io = 4,
    /// The bytes are malformed, truncated or not the format they claim.
    Format = 5,
    /// The call is not valid in the handle's state, or needs an argument the caller did not give.
    Usage = 6,
    /// A Rust panic was caught at the boundary.
    Panic = 7,
    /// The machine stopped on a halt; the last error holds its text.
    Halt = 8,
}

thread_local! {
    static LAST_ERROR: RefCell<Option<CString>> = const { RefCell::new(None) };
}

/// A failed call: the status to return and the message for the last-error slot.
#[derive(Debug)]
pub struct Fail {
    /// The status the call returns.
    pub status: psx_status,
    /// The text `psx_last_error_message` returns.
    pub message: String,
}

impl Fail {
    /// A failure with `status` and `message`.
    pub fn new(status: psx_status, message: impl Into<String>) -> Self {
        Self { status, message: message.into() }
    }

    /// An invalid-argument failure.
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(psx_status::InvalidArgument, message)
    }

    /// A usage failure.
    pub fn usage(message: impl Into<String>) -> Self {
        Self::new(psx_status::Usage, message)
    }

    /// An unsupported failure.
    pub fn unsupported(message: impl Into<String>) -> Self {
        Self::new(psx_status::Unsupported, message)
    }
}

impl From<std::io::Error> for Fail {
    fn from(error: std::io::Error) -> Self {
        Self::new(psx_status::Io, error.to_string())
    }
}

/// Store the message of the last failing call on this thread.
pub fn set_error(message: impl AsRef<str>) {
    let text = message.as_ref().replace('\0', " ");
    LAST_ERROR.with(|slot| *slot.borrow_mut() = CString::new(text).ok());
}

/// Forget the last error, at the start of every fallible call.
pub fn clear_error() {
    LAST_ERROR.with(|slot| *slot.borrow_mut() = None);
}

/// Set the error and return `status`, for checks that run before the panic boundary.
pub fn fail(status: psx_status, message: impl AsRef<str>) -> psx_status {
    set_error(message);
    status
}

/// Clear the error, run `body`, and turn a failure or a panic into a status.
///
/// A panic never crosses the C boundary: it is caught here, its message is stored, and the call
/// reports [`psx_status::Panic`].
pub fn guard(body: impl FnOnce() -> Result<(), Fail>) -> psx_status {
    clear_error();
    match catch_unwind(AssertUnwindSafe(body)) {
        Ok(Ok(())) => psx_status::Ok,
        Ok(Err(error)) => fail(error.status, error.message),
        Err(payload) => fail(psx_status::Panic, format!("panic: {}", panic_text(payload.as_ref()))),
    }
}

fn panic_text(payload: &(dyn Any + Send)) -> &str {
    payload.downcast_ref::<&str>().copied().or_else(|| payload.downcast_ref::<String>().map(String::as_str)).unwrap_or("panic")
}

/// The NUL-terminated message of the last failing call on this thread, or NULL when that call
/// succeeded.
///
/// Borrowed: the pointer is valid until the next call into this library on the same thread.
#[unsafe(no_mangle)]
pub extern "C" fn psx_last_error_message() -> *const c_char {
    LAST_ERROR.with(|slot| slot.borrow().as_ref().map_or(ptr::null(), |text| text.as_ptr()))
}

/// The failure for an EMI archive error.
pub fn map_emi(error: emi::Error) -> Fail {
    match error {
        emi::Error::Io(error) => error.into(),
        emi::Error::Format(error) => map_formats(error),
        emi::Error::EntryOutOfRange { .. } => Fail::new(psx_status::InvalidIndex, error.to_string()),
        other => Fail::new(psx_status::Format, other.to_string()),
    }
}

/// The failure for one of the format library's errors.
pub fn map_formats(error: psx_formats::Error) -> Fail {
    let status = match &error {
        psx_formats::Error::VagOutOfRange { .. } | psx_formats::Error::SepSequenceOutOfRange { .. } => psx_status::InvalidIndex,
        psx_formats::Error::WaveUnsupported(_) => psx_status::Unsupported,
        _ => psx_status::Format,
    };
    Fail::new(status, error.to_string())
}

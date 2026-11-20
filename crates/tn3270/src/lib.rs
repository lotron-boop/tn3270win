//! TN3270 protocol engine (std only, no I/O).
//!
//! * [`telnet::Telnet`]: Telnet option negotiation, TN3270E (RFC 2355) with
//!   fallback to classic TN3270 (RFC 1576), record framing.
//! * [`screen::Screen`]: the 3270 buffer: host writes (commands and
//!   orders), fields and attributes, local editing, inbound Read Modified.
//! * [`session::Session`]: ties the two together. Feed it bytes from the
//!   socket, call its key functions, and send whatever `take_output` returns.
//!
//! The caller owns the socket, so everything here is unit-testable.

pub mod codes;
pub mod ebcdic;
mod ebcdic_tables;
pub mod net;
pub mod query;
pub mod screen;
pub mod session;
pub mod sha256;
pub mod telnet;

pub use ebcdic::CodePage;
pub use screen::{Aid, Color, InputError, Model, Screen};
pub use session::{Session, SessionState};

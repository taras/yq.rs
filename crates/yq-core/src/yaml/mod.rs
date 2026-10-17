//! YAML codec: events from the vendored saphyr parser on the way in, a port of
//! go-yaml's emitter on the way out.

pub mod decode;
pub mod emitter;
pub mod encode;
pub mod resolve;
pub mod saphyr;

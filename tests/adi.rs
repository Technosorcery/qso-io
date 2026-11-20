//! ADI contracts exercised exclusively through public APIs.
#[macro_use]
#[path = "adi/support.rs"]
mod support;
#[path = "adi/fixtures.rs"]
mod fixtures;
#[path = "adi/framing.rs"]
mod framing;
#[path = "adi/permissive.rs"]
mod permissive;
#[path = "adi/strict.rs"]
mod strict;
#[path = "adi/writer.rs"]
mod writer;

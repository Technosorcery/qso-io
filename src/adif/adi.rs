//! Length-delimited ADI reading, writing, and source-aware field lookup.

mod application;
pub mod lookup;
pub mod parser;
pub mod writer;

fn userdef_suffix(name: &str) -> Option<&str> {
    let (prefix, suffix) = name.split_at_checked("USERDEF".len())?;
    (prefix.eq_ignore_ascii_case("USERDEF")
        && !suffix.is_empty()
        && suffix.bytes().all(|byte| byte.is_ascii_digit()))
    .then_some(suffix)
}

fn reserved_user_field_name(name: &str) -> bool {
    name.parse::<super::StandardField>().is_ok()
        || userdef_suffix(name).is_some_and(|digits| digits.bytes().any(|byte| byte != b'0'))
}

pub(crate) fn local(name: &str) -> &str {
    name.rsplit(':').next().unwrap_or(name)
}

pub(crate) fn is_namespace(name: &str) -> bool {
    name == "xmlns" || name.starts_with("xmlns:")
}

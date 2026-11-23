/// The line number or count a header `field` states. Only ASCII digits are a count:
/// Rust's integer parsing also admits a leading `+`, which no diff writes. Rules:
/// docs/spec/architecture.md#input-contract.
fn decimal_count(field: &str) -> Option<usize> {
    let digits = !field.is_empty() && field.bytes().all(|byte| byte.is_ascii_digit());
    if !digits {
        return None;
    }

    field.parse().ok()
}

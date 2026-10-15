pub fn parse_u64_field(field_name: &str, value: &str) -> Result<u64, String> {
    value
        .parse::<u64>()
        .map_err(|error| format!("Invalid string for {field_name}: {error}"))
}

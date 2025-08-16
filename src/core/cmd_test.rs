#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_parse_memory_size() {
        // Test different memory size formats
        assert_eq!(parse_memory_size("100").unwrap(), 100);
        assert_eq!(parse_memory_size("100k").unwrap(), 100 * 1024);
        assert_eq!(parse_memory_size("100K").unwrap(), 100 * 1024);
        assert_eq!(parse_memory_size("100m").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_memory_size("100M").unwrap(), 100 * 1024 * 1024);
        assert_eq!(parse_memory_size("1g").unwrap(), 1024 * 1024 * 1024);
        assert_eq!(parse_memory_size("1G").unwrap(), 1024 * 1024 * 1024);
    }

    #[test]
    fn test_parse_memory_size_invalid() {
        assert!(parse_memory_size("invalid").is_err());
        assert!(parse_memory_size("100x").is_err());
        assert!(parse_memory_size("").is_err());
        assert!(parse_memory_size("-100").is_err());
    }

    #[test]
    fn test_parse_memory_size_edge_cases() {
        assert_eq!(parse_memory_size("0").unwrap(), 0);
        assert_eq!(parse_memory_size("  100m  ").unwrap(), 100 * 1024 * 1024);
    }
}
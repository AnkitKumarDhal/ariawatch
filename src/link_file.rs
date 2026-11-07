use crate::aria2::validate_download_uri;

// this is the neat feature for DDL users,,, it extracts the download links from a download file
// (txt, md, whatever), ignores blank lines, treats lines starting with '#' as comments, and even
// deals with the special characters at the ends of the links (whitespace and UTF-8 BOM chars)
pub(crate) fn parse_link_file(contents: &str) -> Result<Vec<String>, String> {
    let mut links = Vec::new();

    for (index, raw_line) in contents.lines().enumerate() {
        let line_number = index + 1;
        let line = raw_line.trim().trim_start_matches('\u{feff}').trim();

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line.to_ascii_lowercase().starts_with("magnet:?") {
            return Err(format!(
                "Magnet links are not supported in link files (line {line_number}); add them individually"
            ));
        }

        validate_download_uri(line)
            .map_err(|error| format!("Invalid link on line {line_number}: {error}"))?;

        links.push(line.to_owned());
    }

    if links.is_empty() {
        return Err(String::from("Link file contains no downloadable URLs"));
    }

    Ok(links)
}

#[cfg(test)]
mod tests {
    use super::parse_link_file;

    #[test]
    fn parses_supported_download_urls() {
        let contents = "\
http://example.com/file1.zip
https://example.com/file2.zip
ftp://example.com/file3.zip
ftps://example.com/file4.zip
sftp://example.com/file5.zip
";

        let links = parse_link_file(contents).unwrap();

        assert_eq!(links.len(), 5);
    }

    #[test]
    fn ignores_blank_lines_and_comments() {
        let contents = "\
# First download

https://example.com/file1.zip

    # Second download
https://example.com/file2.zip

";

        let links = parse_link_file(contents).unwrap();

        assert_eq!(
            links,
            vec![
                String::from("https://example.com/file1.zip"),
                String::from("https://example.com/file2.zip"),
            ]
        );
    }

    #[test]
    fn trims_whitespace_and_handles_utf8_bom() {
        let contents = "\u{feff}  https://example.com/file.zip  \r\n";

        let links = parse_link_file(contents).unwrap();

        assert_eq!(links, vec![String::from("https://example.com/file.zip")]);
    }

    #[test]
    fn preserves_order_and_duplicate_links() {
        let contents = "\
https://example.com/first.zip
https://example.com/second.zip
https://example.com/first.zip
";

        let links = parse_link_file(contents).unwrap();

        assert_eq!(
            links,
            vec![
                String::from("https://example.com/first.zip"),
                String::from("https://example.com/second.zip"),
                String::from("https://example.com/first.zip"),
            ]
        );
    }

    #[test]
    fn rejects_unsupported_uri_schemes_with_line_number() {
        let contents = "\
https://example.com/file.zip
file:///etc/passwd
";

        let error = parse_link_file(contents).unwrap_err();

        assert!(error.contains("line 2"));
        assert!(error.contains("Unsupported URI scheme"));
    }

    #[test]
    fn rejects_magnet_links_with_line_number() {
        let contents = "\
https://example.com/file.zip
magnet:?xt=urn:btih:0123456789abcdef
";

        let error = parse_link_file(contents).unwrap_err();

        assert!(error.contains("line 2"));
        assert!(error.contains("Magnet links are not supported"));
    }

    #[test]
    fn rejects_urls_containing_whitespace() {
        let error = parse_link_file("https://example.com/file with spaces.zip").unwrap_err();

        assert!(error.contains("whitespace"));
    }

    #[test]
    fn rejects_empty_or_comment_only_files() {
        assert!(parse_link_file("").is_err());
        assert!(parse_link_file("\n\n").is_err());
        assert!(parse_link_file("# comment\n\n # another comment").is_err());
    }
}

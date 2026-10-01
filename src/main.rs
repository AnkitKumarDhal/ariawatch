fn describe_release(version: &str) -> String {
    let stage = match version {
        "0.0.1" => "bootstrap",
        "0.0.2" => "foundations",
        _ => "future release",
    };

    format!("ariawatch v{version} - {stage}")
}

fn format_releases(version: &str, stage: &str) -> String {
    format!("ariawatch v{version} - {stage}")
}

fn main() {
    let mut versions = Vec::new();
    let mut stage = Vec::new();

    versions.push("0.0.1");
    versions.push("0.0.2");
    versions.push("0.0.3");

    stage.push("bootstrap");
    stage.push("foundations");
    stage.push("future release");

    for version in &versions {
        println!("{version}");
    }

    for version in &versions {
        let msg = describe_release(version);
        println!("{msg}")
    }

    for i in 0..versions.len() {
        let msg = format_releases(versions[i], stage[i]);
        println!("{msg}")
    }
}

#[test]
fn test_format_releases() {
    let version = "0.0.1";
    let stage = "bootstrap";
    let expected = "ariawatch v0.0.1 - bootstrap";
    let result = format_releases(version, stage);
    assert_eq!(result, expected);
}

fn describe_release(version: &str) -> String {
    let stage = match version {
        "0.0.1" => "bootstrap",
        "0.0.2" => "foundations",
        _ => "future release",
    };

    format!("ariawatch v{version} - {stage}")
}

fn main() {
    let mut versions = Vec::new();

    versions.push("0.0.1");
    versions.push("0.0.2");
    versions.push("0.0.3");
    versions.push("0.0.4");

    for version in &versions {
        println!("{version}");
    }

    for version in &versions {
        let msg = describe_release(version);
        println!("{msg}")
    }
}

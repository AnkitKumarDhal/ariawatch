fn describe_release(version: &str) -> String {
    let stage = match version {
        "0.0.1" => "bootstrap",
        "0.0.2" => "foundations",
        _ => "future release",
    };

    format!("ariawatch v{version} - {stage}")
}

fn main() {
    let versions = ["0.0.1", "0.0.2", "0.0.3"];

    for version in &versions {
        let msg = describe_release(version);
        println!("{msg}")
    }
}

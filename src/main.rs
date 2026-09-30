fn print_version(name: &str, version: &str) {
    println!("{name} v{version}");

    let stage = match version {
        "0.0.1" => "bootstrap",
        "0.0.2" => "foundations",
        _ => "future release",
    };

    println!("stage: {stage}");
}

fn main() {
    let name = "ariawatch";
    let versions = ["0.0.1", "0.0.2", "0.0.3"];

    for version in &versions {
        print_version(name, version);
        println!("")
    }
}

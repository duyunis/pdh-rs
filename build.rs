use std::error::Error;
use std::process::Command;
use chrono::Local;

struct EnvCommand(&'static str, Vec<&'static str>);

fn set_build_info() -> Result<(), Box<dyn Error>> {
    println!(
        "cargo:rustc-env=COMPILE_TIME={}",
        Local::now().format("%F %T")
    );
    let entries = vec![
        EnvCommand("RUSTC_VERSION", vec!["rustc", "--version"]),
    ];
    for e in entries {
        let output = Command::new(e.1[0]).args(&e.1[1..]).output()?.stdout;
        println!("cargo:rustc-env={}={}", e.0, String::from_utf8(output)?);
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn Error>> {
    set_build_info()?;
    Ok(())
}
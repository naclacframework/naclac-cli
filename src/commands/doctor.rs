use std::process::Command;

pub fn execute() {
    println!("🩺 Running Naclac Environment Diagnosis...");
    
    let checks = [
        ("Naclac CLI Router", "cargo-build-sbf", vec!["--version"]),
        ("Rust Compiler", "rustc", vec!["--version"]),
        ("Solana CLI", "solana", vec!["--version"]),
        ("NodeJS", "node", vec!["--version"]),
        ("Yarn Package Manager", "yarn", vec!["--version"]),
    ];

    let mut all_good = true;

    for (name, cmd, args) in checks.iter() {
        match Command::new(cmd).args(args).output() {
            Ok(output) if output.status.success() => {
                let stdout = String::from_utf8_lossy(&output.stdout);
                let version = stdout.lines().next().unwrap_or("Unknown version");
                println!("  ✅ {} ({}): {}", name, cmd, version);
            }
            _ => {
                println!("  ❌ {} ({}) not found or failed to execute.", name, cmd);
                all_good = false;
            }
        }
    }

    if all_good {
        println!("\n🎉 Your environment is perfectly configured for Naclac!");
    } else {
        println!("\n⚠️  Please install the missing dependencies to ensure seamless builds.");
    }
}

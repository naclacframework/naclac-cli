use std::process::Command;
use std::fs;
use toml::Value;

pub fn execute(program_id: Option<&str>) {
    let current_dir = std::env::current_dir().unwrap();
    let toml_path = if current_dir.join("Naclac.toml").exists() {
        current_dir.join("Naclac.toml")
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../../Naclac.toml").canonicalize().unwrap()
    } else {
        eprintln!("❌ Not a Naclac workspace. Please run this inside a naclac project or specify a target '--url' manually via raw solana CLI.");
        std::process::exit(1);
    };

    let content = fs::read_to_string(&toml_path).unwrap_or_default();
    let parsed: Value = toml::from_str(&content).unwrap_or(Value::Table(Default::default()));

    let cluster = parsed
        .get("provider")
        .and_then(|p| p.get("cluster"))
        .and_then(|c| c.as_str())
        .unwrap_or("localnet");

    let network_flag = match cluster {
        "localnet" => "http://localhost:8899",
        "devnet" => "devnet",
        "testnet" => "testnet",
        _ => cluster,
    };

    let mut cmd = Command::new("solana");
    cmd.arg("logs");
    
    if let Some(pid) = program_id {
        println!("📡 Tailing logs for program: {} on {}", pid, cluster);
        cmd.arg(pid);
    } else {
        println!("📡 Tailing all logs on {}", cluster);
    }

    cmd.arg("--url").arg(network_flag);

    match cmd.status() {
        Ok(s) if !s.success() => println!("⚠️ Stream exited. Is the cluster running?"),
        Err(e) => println!("❌ Failed to capture logs: {}", e),
        _ => {}
    }
}

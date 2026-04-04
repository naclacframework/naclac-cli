use std::process::Command;
use std::fs;
use toml::Value;

pub fn execute(amount: f64) {
    let current_dir = std::env::current_dir().unwrap();
    let toml_path = if current_dir.join("Naclac.toml").exists() {
        current_dir.join("Naclac.toml")
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../../Naclac.toml").canonicalize().unwrap()
    } else {
        eprintln!("❌ Not a Naclac workspace.");
        std::process::exit(1);
    };

    let content = fs::read_to_string(&toml_path).unwrap_or_default();
    let parsed: Value = toml::from_str(&content).unwrap_or(Value::Table(Default::default()));

    let cluster = parsed
        .get("provider")
        .and_then(|p| p.get("cluster"))
        .and_then(|c| c.as_str())
        .unwrap_or("devnet");

    let network_flag = match cluster {
        "localnet" => "http://127.0.0.1:8899",
        "devnet" => "https://api.devnet.solana.com",
        "testnet" => "https://api.testnet.solana.com",
        _ => cluster, 
    };

    println!("🪂 Requesting airdrop of {} SOL to cluster: {}", amount, cluster);
    
    let status = Command::new("solana")
        .arg("airdrop")
        .arg(amount.to_string())
        .arg("--url")
        .arg(network_flag)
        .status();

    match status {
        Ok(s) if s.success() => println!("✅ Airdrop successful!"),
        Ok(_) | Err(_) => println!("❌ Airdrop failed. Please check network connectivity or rate limits."),
    }
}

use std::fs;
use std::process::Command;

pub fn execute(program_id: Option<&str>) {
    let current_dir = std::env::current_dir().unwrap();

    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml. Please run from within a Naclac workspace.");
        std::process::exit(1);
    };

    let deploy_dir = workspace_root.join("target/deploy");
    if !deploy_dir.exists() {
        eprintln!("❌ Error: 'target/deploy' directory not found. Please run `naclac build` first.");
        std::process::exit(1);
    }

    // --- Parse Naclac.toml for Cluster and Wallet ---
    let toml_path = workspace_root.join("Naclac.toml");
    let toml_content = fs::read_to_string(&toml_path).unwrap();

    let mut cluster_setting = "localnet".to_string();
    let mut wallet_setting = "~/.config/solana/id.json".to_string();

    for line in toml_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("cluster =") {
            if let Some(val) = trimmed.split('"').nth(1) {
                cluster_setting = val.to_string();
            }
        }
        if trimmed.starts_with("wallet =") {
            if let Some(val) = trimmed.split('"').nth(1) {
                wallet_setting = val.to_string();
            }
        }
    }

    // Map cluster names to actual RPC URLs
    let cluster_url = match cluster_setting.as_str() {
        "localnet" => "http://127.0.0.1:8899",
        "devnet" => "https://api.devnet.solana.com",
        "testnet" => "https://api.testnet.solana.com",
        "mainnet" => "https://api.mainnet-beta.solana.com",
        custom => custom, // If they pasted a QuickNode/Helius URL, use it directly!
    };

    // Resolve the '~' in the wallet path to the actual Home Directory
    let home_dir = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_default();
    let expanded_wallet = wallet_setting.replace("~", &home_dir);

    let mut so_files = Vec::new();
    for entry in fs::read_dir(&deploy_dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().unwrap_or_default() == "so" {
            let program_name = path.file_stem().unwrap().to_str().unwrap();
            let is_target = program_id.map_or(true, |tgt| program_name == tgt);
            if is_target {
                so_files.push(path);
            }
        }
    }

    if so_files.is_empty() {
        eprintln!("❌ Error: No compiled .so programs found. Please run `naclac build` first.");
        std::process::exit(1);
    }

    // --- Anchor-Parity Console Output ---
    for so_file in so_files {
        let program_name = so_file.file_stem().unwrap().to_str().unwrap();

        println!("Deploying cluster: {}", cluster_url);
        println!("Upgrade authority: {}", expanded_wallet);
        println!("Deploying program \"{}\"...", program_name);
        println!("Program path: {}\n", so_file.display());

        // Execute the native Solana deploy command with the exact URL and Wallet.
        // NOTE: If this fails midway, running it again will automatically RESUME 
        // because the Solana CLI detects the existing buffer account for this keypair!
        let mut child = Command::new("solana")
            .arg("program")
            .arg("deploy")
            .arg(&so_file)
            .arg("--url")
            .arg(cluster_url)
            .arg("--keypair")
            .arg(&expanded_wallet)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("Failed to execute solana program deploy");

        let status = child.wait().expect("Failed to wait on deployment");

        if status.success() {
            println!("\n✅ Successfully deployed '{}'!", program_name);
        } else {
            eprintln!("\n❌ Failed to deploy '{}'. Run `naclac deploy` again to resume the deployment.", program_name);
            std::process::exit(1);
        }
    }
}
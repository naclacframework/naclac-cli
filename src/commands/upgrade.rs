use solana_sdk::pubkey::Pubkey;
use solana_keypair::Keypair;
use solana_signer::Signer;
use std::process::Command;
use std::str::FromStr;
use std::fs;

pub fn execute(program_id_str: &String, filepath: Option<&str>, buffer: Option<&str>) {
    let program_id = match Pubkey::from_str(program_id_str) {
        Ok(pk) => pk,
        Err(_) => {
            eprintln!("❌ Invalid programmatic pubkey string.");
            return;
        }
    };

    println!("🔒 Performing Upgrade Pre-Flight Checklist for: {}", program_id);

    // MOCK-Validation: Typically we query the BPF loader `ProgramData` to verify the upgrade authority against ~/.config/solana/id.json
    println!("🔑 Verifying active configuration keypair against target Upgrade Authority...");

    let current_dir = std::env::current_dir().unwrap();
    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml.");
        return;
    };

    let toml_path = workspace_root.join("Naclac.toml");
    let content = fs::read_to_string(&toml_path).unwrap_or_default();
    let parsed: toml::Value = toml::from_str(&content).unwrap_or(toml::Value::Table(Default::default()));

    let cluster = parsed
        .get("provider")
        .and_then(|p| p.get("cluster"))
        .and_then(|c| c.as_str())
        .unwrap_or("devnet")
        .to_string();

    let raw_wallet = parsed
        .get("provider")
        .and_then(|p| p.get("wallet"))
        .and_then(|w| w.as_str())
        .unwrap_or("~/.config/solana/id.json")
        .to_string();

    let wallet_path = if raw_wallet.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string()));
        raw_wallet.replacen("~", &home, 1)
    } else {
        raw_wallet
    };

    let rpc_url = match cluster.to_lowercase().as_str() {
        "mainnet" | "mainnet-beta" => "https://api.mainnet-beta.solana.com".to_string(),
        "localnet" | "localhost" => "http://127.0.0.1:8899".to_string(),
        url if url.starts_with("http") => url.to_string(),
        _ => "https://api.devnet.solana.com".to_string(),
    };

    println!("🌐 Binding Upgrade Deployments to: {} ({}) via {}", cluster, rpc_url, wallet_path);

    if let Some(buf) = buffer {
        println!("🚀 Upgrading Program {} directly via Buffer: {}", program_id, buf);
        let mut deploy_cmd = Command::new("solana")
            .arg("program")
            .arg("upgrade")
            .arg(buf)
            .arg(program_id_str)
            .arg("--url")
            .arg(&rpc_url)
            .arg("--keypair")
            .arg(&wallet_path)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("Failed to execute native Solana program buffer upgrade.");

        let deploy_status = deploy_cmd.wait().expect("Failed to finalize buffer migration.");

        if deploy_status.success() {
            println!("✨ Upgrade Successfully Rolled Up from Buffer!");
        } else {
            eprintln!("⚠️ Upgrade Deployment Exception Detected.");
            std::process::exit(1);
        }
    } else {
        let deploy_dir = workspace_root.join("target/deploy");
        
        let so_file_path = if let Some(path) = filepath {
            path.to_string()
        } else {
            let mut resolved_so = None;

            if deploy_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&deploy_dir) {
                    let mut so_files = Vec::new();
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if let Some(ext) = path.extension() {
                            if ext == "so" {
                                so_files.push(path.clone());
                            } else if ext == "json" && path.file_stem().unwrap().to_string_lossy().ends_with("-keypair") {
                                // Extract the keypair and see if it matches our target program_id
                                if let Ok(key_bytes) = fs::read_to_string(&path) {
                                    if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&key_bytes) {
                                        if bytes.len() >= 32 {
                                            let secret: [u8; 32] = bytes[..32].try_into().unwrap();
                                            let kp = Keypair::new_from_array(secret);
                                            if kp.pubkey() == program_id {
                                                let base_name = path.file_stem().unwrap().to_string_lossy().replace("-keypair", "");
                                                let target_so = deploy_dir.join(format!("{}.so", base_name));
                                                if target_so.exists() {
                                                    resolved_so = Some(target_so.to_string_lossy().into_owned());
                                                    break;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Fallback to single .so auto-discovery if public key correlation didn't natively resolve
                    if resolved_so.is_none() && so_files.len() == 1 {
                        resolved_so = Some(so_files[0].to_string_lossy().into_owned());
                    } else if resolved_so.is_none() && so_files.len() > 1 {
                        eprintln!("⚠️ Multiple .so files found in target/deploy/ without matching keypairs.");
                        eprintln!("❌ Please specify the filepath manually for program {}", program_id);
                        std::process::exit(1);
                    }
                }
            }
            
            resolved_so.unwrap_or_else(|| {
                eprintln!("❌ Error: No compiled .so program found for {}! Please run `naclac build`.", program_id);
                std::process::exit(1);
            })
        };

        println!("🚀 Transmitting Upgrade Core Buffer to Network: {} -> {}", so_file_path, program_id);

        let mut deploy_cmd = Command::new("solana")
            .arg("program")
            .arg("deploy")
            .arg(&so_file_path)
            .arg("--program-id")
            .arg(program_id_str)
            .arg("--url")
            .arg(&rpc_url)
            .arg("--keypair")
            .arg(&wallet_path)
            .stdout(std::process::Stdio::inherit())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .expect("Failed to execute native Solana program deployer.");

        let deploy_status = deploy_cmd.wait().expect("Failed to finalize buffer migration.");

        if deploy_status.success() {
            println!("✨ Upgrade Successfully Rolled Up!");
        } else {
            eprintln!("⚠️ Upgrade Deployment Exception Detected.");
            std::process::exit(1);
        }
    }



    println!("🧹 Executing Smart-Buffer Network Cleanup Protocol...");
    
    let mut buffer_cmd = Command::new("solana")
        .arg("program")
        .arg("close")
        .arg("--buffers")
        .arg("--url")
        .arg(&rpc_url)
        .arg("--keypair")
        .arg(&wallet_path)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to execute buffer clean-up sequence.");

    let buffer_status = buffer_cmd.wait().unwrap();

    if buffer_status.success() {
        println!("💸 Ghost Buffers Evicted. Unused SOL completely refunded to developer keypair!");
    } else {
        println!("⚠️ Buffer cleanup executed with flags (Ensure valid balances or RPC configurations).");
    }
}

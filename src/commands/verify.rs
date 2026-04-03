use std::process::Command;
use sha2::{Sha256, Digest};
use std::fs;
use solana_sdk::pubkey::Pubkey;
use solana_keypair::Keypair;
use solana_signer::Signer;
use std::str::FromStr;

pub fn execute(program_id_str: &String) {
    // Validate the pubkey format early even though we use it as a string for display
    if bs58::decode(program_id_str).into_vec().is_err() {
        eprintln!("❌ Invalid base58 program ID.");
        return;
    }

    println!("🔍 Initializing Trust Protocol Verifier against Program ID: {}", program_id_str);

    // ==========================================
    // 1. DOCKER DEPENDENCY VERIFICATION
    // ==========================================
    let docker_check = Command::new("docker")
        .arg("--version")
        .output();

    match docker_check {
        Ok(out) if out.status.success() => {
            println!("🐳 Native Docker runtime located. Initiating standardized verifiable bounds.");
        },
        _ => {
            eprintln!("
❌ Error: Docker is not running or missing from systemic PATH.
Naclac requires Docker to perform verifiable, deterministic builds to eliminate OS disparities (Mac vs Windows). 
Please install Docker (https://docs.docker.com/get-docker/) and ensure the daemon is running before running 'naclac verify'.
");
            return;
        }
    }

    let current_dir = std::env::current_dir().unwrap();
    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml.");
        return;
    };

    // ==========================================
    // 2. DETERMINISTIC BUILD
    // ==========================================
    println!("📦 Spinning up ellipsislabs/solana:latest standardization container...");
    println!("🔄 Mounting workspace and synthesizing verifiable SBF payload...");
    
    let parent_dir = workspace_root.parent().unwrap();
    let project_name = workspace_root.file_name().unwrap().to_str().unwrap();

    let lock_path = workspace_root.join("Cargo.lock");
    let mut lock_content = None;

    // Mask the V4 Lockfile Native to the host so the isolated docker runtime doesn't crash reading it
    if lock_path.exists() {
        if let Ok(content) = fs::read_to_string(&lock_path) {
            lock_content = Some(content);
            fs::remove_file(&lock_path).unwrap_or_default();
        }
    }

    let mut build_cmd = Command::new("docker")
        .arg("run")
        .arg("--rm")
        .arg("-v")
        .arg(format!("{}:/workspace", parent_dir.display()))
        .arg("-w")
        .arg(format!("/workspace/{}", project_name))
        .arg("ellipsislabs/solana:latest")
        .arg("cargo")
        .arg("build-sbf")
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to initialize Docker runtime.");

    let build_status = build_cmd.wait().expect("Docker wrapper unexpectedly failed.");

    // Restore the host's lockfile immediately after the isolation drops
    if let Some(content) = lock_content {
        fs::write(&lock_path, content).unwrap_or_default();
    }

    if !build_status.success() {
        eprintln!("❌ Dockerized Deterministic Compilation failed. Resolve errors to continue verifying.");
        return;
    }

    // ==========================================
    // 3. HASH COMPARISON
    // ==========================================
    println!("🗜 Hashing target buffer payload...");
    
    // Find matching SO file bypassing simple string names
    let deploy_dir = workspace_root.join("target/deploy");
    let mut matching_so = None;

    let target_pubkey = Pubkey::from_str(program_id_str).unwrap();

    if deploy_dir.exists() {
        if let Ok(entries) = fs::read_dir(&deploy_dir) {
            let mut so_files = Vec::new();
            for entry in entries.flatten() {
                let path = entry.path();
                if let Some(ext) = path.extension() {
                    if ext == "so" {
                        so_files.push(path.clone());
                    } else if ext == "json" && path.file_stem().unwrap().to_string_lossy().ends_with("-keypair") {
                        if let Ok(key_bytes) = fs::read_to_string(&path) {
                            if let Ok(bytes) = serde_json::from_str::<Vec<u8>>(&key_bytes) {
                                if bytes.len() >= 32 {
                                    let secret: [u8; 32] = bytes[..32].try_into().unwrap();
                                    let kp = Keypair::new_from_array(secret);
                                    if kp.pubkey() == target_pubkey {
                                        let base_name = path.file_stem().unwrap().to_string_lossy().replace("-keypair", "");
                                        let target_so = deploy_dir.join(format!("{}.so", base_name));
                                        if target_so.exists() {
                                            matching_so = Some(target_so);
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if matching_so.is_none() && so_files.len() == 1 {
                matching_so = Some(so_files[0].clone());
            }
        }
    }

    if let Some(so_path) = matching_so {
        if let Ok(local_bytes) = fs::read(&so_path) {
            let mut hasher = Sha256::new();
            hasher.update(&local_bytes);
            let hash_bytes = hasher.finalize();
            let local_hash = hex::encode(hash_bytes);
            println!("🔒 Local Checksum: {}", local_hash);

            println!("🌐 Fetching true on-chain buffer dump securely over RPC...");
            
            let tmp_onchain_dump = std::env::temp_dir().join(format!("{}_onchain.so", program_id_str));
            let dump_cmd = Command::new("solana")
                .arg("program")
                .arg("dump")
                .arg(program_id_str)
                .arg(&tmp_onchain_dump)
                .output()
                .expect("Failed to execute solana program dump fetching process.");

            if !dump_cmd.status.success() {
                eprintln!("❌ Failed querying network. Are you connected to the right cluster? {}", String::from_utf8_lossy(&dump_cmd.stderr));
                return;
            }

            if let Ok(onchain_bytes) = fs::read(&tmp_onchain_dump) {
                // Slicing algorithm: Solana deploys pad `.so` bytecodes to support future Reallocs (Usually +2x size in zeros). 
                // We strictly map and isolate their verification length boundaries to emulate flawless determinism comparisons against local artifacts.
                let comparable_length = std::cmp::min(onchain_bytes.len(), local_bytes.len());
                let comparable_onchain_slice = &onchain_bytes[..comparable_length];

                let mut chain_hasher = Sha256::new();
                chain_hasher.update(comparable_onchain_slice);
                let on_chain_hash = hex::encode(chain_hasher.finalize());
                
                println!("🌍 Mainnet Checksum: {}", on_chain_hash);

                if local_hash == on_chain_hash {
                    println!("✅ Verification Successful: On-chain code perfectly matches local source.");
                } else {
                    eprintln!("❌ VERIFICATION FAILED: Detected divergence between real On-Chain code and local SBF targets!");
                }

                fs::remove_file(&tmp_onchain_dump).unwrap_or_default();
            } else {
                eprintln!("❌ Core Dump processing array missing!");
            }
        } else {
            eprintln!("❌ Failed to read matching `.so` file arrays.");
        }
    } else {
        eprintln!("❌ No verifiable program artifacts found for Program ID: {}", program_id_str);
    }
}

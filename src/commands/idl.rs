use crate::IdlAction;
use flate2::write::ZlibEncoder;
use flate2::Compression;
use solana_rpc_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;
use std::fs;
use std::io::Write;
use std::str::FromStr;
use toml::Value;

pub fn execute(action: &IdlAction) {
    match action {
        IdlAction::Init { program_id } => init_idl(program_id.as_ref().unwrap()),
    }
}

struct NaclacConfig {
    cluster: String,
    wallet_path: String,
    rpc_url: String,
}

fn load_naclac_config() -> Option<NaclacConfig> {
    let current_dir = std::env::current_dir().unwrap();

    let toml_path = if current_dir.join("Naclac.toml").exists() {
        current_dir.join("Naclac.toml")
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir
            .join("../../Naclac.toml")
            .canonicalize()
            .unwrap()
    } else {
        eprintln!("❌ Not a Naclac workspace. Run `naclac init` to initialize a new one.");
        return None;
    };

    let content = match fs::read_to_string(&toml_path) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("❌ Found Naclac.toml but could not read it.");
            return None;
        }
    };

    let parsed: Value = match toml::from_str(&content) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("❌ Naclac.toml is malformed. Please check its format.");
            return None;
        }
    };

    // Extract [provider] cluster
    let cluster = parsed
        .get("provider")
        .and_then(|p| p.get("cluster"))
        .and_then(|c| c.as_str())
        .unwrap_or("devnet")
        .to_string();

    // Extract [provider] wallet
    let raw_wallet = parsed
        .get("provider")
        .and_then(|p| p.get("wallet"))
        .and_then(|w| w.as_str())
        .unwrap_or("~/.config/solana/id.json")
        .to_string();

    // Expand ~ to actual home directory
    let wallet_path = if raw_wallet.starts_with("~/") {
        let home = std::env::var("HOME").unwrap_or_else(|_| {
            // fallback for Windows/WSL
            std::env::var("USERPROFILE").unwrap_or_else(|_| ".".to_string())
        });
        raw_wallet.replacen("~", &home, 1)
    } else {
        raw_wallet
    };

    // Map cluster name to RPC URL
    let rpc_url = match cluster.to_lowercase().as_str() {
        "mainnet" | "mainnet-beta" => "https://api.mainnet-beta.solana.com".to_string(),
        "devnet" => "https://api.devnet.solana.com".to_string(),
        "testnet" => "https://api.testnet.solana.com".to_string(),
        "localnet" | "localhost" => "http://127.0.0.1:8899".to_string(),
        // If they put a raw URL directly in the toml, use it as-is
        url if url.starts_with("http") => url.to_string(),
        other => {
            eprintln!(
                "⚠️  Unknown cluster '{}' in Naclac.toml, defaulting to devnet.",
                other
            );
            "https://api.devnet.solana.com".to_string()
        }
    };

    println!(
        "📋 Naclac.toml loaded → cluster: {}, wallet: {}",
        cluster, wallet_path
    );

    Some(NaclacConfig {
        cluster,
        wallet_path,
        rpc_url,
    })
}

fn load_keypair(wallet_path: &str) -> Option<Keypair> {
    let key_bytes = match fs::read_to_string(wallet_path) {
        Ok(content) => content,
        Err(_) => {
            eprintln!("❌ Could not read wallet keypair at: {}", wallet_path);
            eprintln!("   Make sure the path in Naclac.toml [provider] wallet is correct.");
            return None;
        }
    };

    let bytes: Vec<u8> = match serde_json::from_str(&key_bytes) {
        Ok(b) => b,
        Err(_) => {
            eprintln!("❌ Wallet file is not a valid Solana keypair JSON.");
            return None;
        }
    };

    let secret: [u8; 32] = bytes[..32].try_into().expect("❌ Keypair bytes too short.");
    Some(Keypair::new_from_array(secret))
}

fn init_idl(program_id_str: &String) {
    // 1. Load Naclac.toml config
    let config = match load_naclac_config() {
        Some(c) => c,
        None => return,
    };

    // 2. Parse program ID
    let program_id = match Pubkey::from_str(program_id_str) {
        Ok(pk) => pk,
        Err(_) => {
            eprintln!("❌ Invalid programmatic pubkey string.");
            return;
        }
    };


    println!("🚀 Initialize deterministic IDL context structure for: {}", program_id);

    // 3. Derive IDL PDA
    let seed = b"anchor:idl";
    let (idl_pda, _bump) = Pubkey::find_program_address(&[seed, program_id.as_ref()], &program_id);
    println!("🔐 Derived Anchor-Compatible IDL PDA: {}", idl_pda);

    // 4. Find and load IDL file
    let current_dir = std::env::current_dir().unwrap();
    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else {
        current_dir.join("../..").canonicalize().unwrap()
    };

    let idl_dir = workspace_root.join("target/idl");
    let mut idl_content = None;

    if idl_dir.exists() {
        for entry in fs::read_dir(&idl_dir).unwrap() {
            if let Ok(entry) = entry {
                if let Ok(content) = fs::read_to_string(entry.path()) {
                    if content.contains(program_id_str) {
                        idl_content = Some(content);
                        break;
                    }
                }
            }
        }
    }

    let payload = match idl_content {
        Some(c) => c,
        None => {
            eprintln!("❌ Error: Could not find a compiled IDL matched to this program.");
            return;
        }
    };

    // 5. Compress IDL
    println!("🗜  Compressing local target IDL via exact Zlib/deflate standard...");
    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder.write_all(payload.as_bytes()).unwrap();
    let compressed_bytes = encoder.finish().unwrap();

    let storage_length = 8 + 32 + 4 + compressed_bytes.len();

    println!(
        "✅ Compression Output: ~{} bytes mapped from raw json.",
        compressed_bytes.len()
    );

    println!("💰 IDL Transaction Configured: Rent size evaluation mapped to length: {}.", storage_length);

    // 6. Load keypair from wallet path in Naclac.toml
    let payer = match load_keypair(&config.wallet_path) {
        Some(kp) => kp,
        None => return,
    };


    // 7. Connect to RPC
    println!("🌐 Connecting to cluster: {} ({})", config.cluster, config.rpc_url);
    let client = RpcClient::new(config.rpc_url.clone());

    // 8. Check if IDL account already exists
    let idl_exists = client.get_account(&idl_pda).is_ok();

    if idl_exists {
        println!("⚠️  IDL account already exists at {}. Bypassing initialization to perform idempotent buffer upgrade...", idl_pda);
    } else {
        // 9. Calculate rent
        let rent: u64 = match client.get_minimum_balance_for_rent_exemption(storage_length) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("❌ Failed to fetch rent exemption amount: {}", e);
                return;
            }
        };

        println!("💸 Rent required: {} lamports ({:.6} SOL)", rent, rent as f64 / 1_000_000_000_f64);

        // 10. Check payer balance
        let balance = match client.get_balance(&payer.pubkey()) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("❌ Failed to fetch wallet balance: {}", e);
                return;
            }
        };

        if balance < rent {
            eprintln!(
                "❌ Insufficient balance. You have {} lamports but need {} lamports.",
                balance, rent
            );
            eprintln!("   Run `solana airdrop 1` if you are on devnet.");
            return;
        }

        println!("🔄 Pre-flight checklist completed. Sending initialization transaction...");

        // 11. Build and send create account transaction
        let recent_blockhash = match client.get_latest_blockhash() {
            Ok(bh) => bh,
            Err(e) => {
                eprintln!("❌ Failed to fetch latest blockhash: {}", e);
                return;
            }
        };

        let idl_create_disc = solana_sdk::hash::hash(b"naclac:idl_create").to_bytes();
        let mut init_data = idl_create_disc[0..8].to_vec();
        init_data.extend_from_slice(&rent.to_le_bytes());
        init_data.extend_from_slice(&(storage_length as u64).to_le_bytes());

        let create_ix = solana_sdk::instruction::Instruction {
            program_id,
            accounts: vec![
                solana_sdk::instruction::AccountMeta::new(payer.pubkey(), true),
                solana_sdk::instruction::AccountMeta::new(idl_pda, false),
                solana_sdk::instruction::AccountMeta::new_readonly(solana_system_interface::program::id(), false),
            ],
            data: init_data,
        };

        let tx = Transaction::new_signed_with_payer(
            &[create_ix],
            Some(&payer.pubkey()),
            &[&payer],
            recent_blockhash,
        );

        match client.send_and_confirm_transaction(&tx) {
            Ok(sig) => {
                println!("✅ IDL initialize transaction successful!");
                println!("📝 Transaction Signature: {}", sig);
                println!("🔗 View on Explorer: https://explorer.solana.com/tx/{}?cluster={}", sig, config.cluster);
            }
            Err(e) => {
                eprintln!("❌ Transaction failed: {}", e);
                return; // Exit if init fails
            }
        }
    }

    // 12. Write compressed IDL data in chunks
    // The exact Anchor bytes payload structure: discriminator(8) + authority(32) + length(4) + payload(N)
    let idl_write_disc = solana_sdk::hash::hash(b"naclac:idl_write").to_bytes();
    let anchor_disc = solana_sdk::hash::hash(b"account:IdlAccount").to_bytes();

    let mut full_payload = vec![0u8; 8];
    full_payload.copy_from_slice(&anchor_disc[0..8]);
    full_payload.extend_from_slice(payer.pubkey().as_ref());
    full_payload.extend_from_slice(&(compressed_bytes.len() as u32).to_le_bytes());
    full_payload.extend_from_slice(&compressed_bytes);

    let chunk_size = 600;
    let total_len = full_payload.len();
    let mut offset = 0;

    println!("🔄 Starting chunked upload for {} bytes...", total_len);

    while offset < total_len {
        let end = std::cmp::min(offset + chunk_size, total_len);
        let chunk = &full_payload[offset..end];

        let mut write_data = idl_write_disc[0..8].to_vec();
        write_data.extend_from_slice(&(offset as u32).to_le_bytes());
        write_data.extend_from_slice(chunk);

        let write_ix = solana_sdk::instruction::Instruction {
            program_id,
            accounts: vec![
                solana_sdk::instruction::AccountMeta::new(payer.pubkey(), true),
                solana_sdk::instruction::AccountMeta::new(idl_pda, false),
            ],
            data: write_data,
        };

        let current_blockhash = match client.get_latest_blockhash() {
            Ok(bh) => bh,
            Err(e) => {
                eprintln!("❌ Failed to fetch latest blockhash for chunk: {}", e);
                return;
            }
        };

        let chunk_tx = Transaction::new_signed_with_payer(
            &[write_ix],
            Some(&payer.pubkey()),
            &[&payer],
            current_blockhash,
        );

        match client.send_and_confirm_transaction(&chunk_tx) {
            Ok(_) => {
                println!("✅ Programmed confirmed onchain: {}/{} bytes", end, total_len);
            }
            Err(e) => {
                eprintln!("❌ Write chunk failed at offset {}: {}", offset, e);
                return;
            }
        }
        offset += chunk_size;
    }

    println!("🎉 IDL successfully deployed and is 100% Anchor-Compatible!");
}
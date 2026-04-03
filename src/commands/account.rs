use solana_rpc_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::fs;
use std::str::FromStr;
use sha2::{Digest, Sha256};
use serde_json::{Value, Map};
use bs58;

// Re-defining parts of IDL for decoding
use serde::Deserialize;

#[derive(Deserialize, Debug)]
struct Idl {
    accounts: Vec<IdlAccountDef>,
}

#[derive(Deserialize, Debug)]
struct IdlAccountDef {
    name: String,
    #[serde(rename = "type")]
    ty: IdlTypeDef,
}

#[derive(Deserialize, Debug)]
struct IdlTypeDef {
    fields: Vec<IdlField>,
}

#[derive(Deserialize, Debug)]
struct IdlField {
    name: String,
    #[serde(rename = "type")]
    ty: Value,
}

fn get_rpc_url() -> String {
    let home = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")).unwrap_or_else(|_| ".".to_string());
    let config_path = format!("{}/.config/solana/cli/config.yml", home);
    if let Ok(content) = std::fs::read_to_string(&config_path) {
        for line in content.lines() {
            if line.starts_with("json_rpc_url:") {
                return line.replace("json_rpc_url:", "").trim().to_string();
            }
        }
    }
    "http://127.0.0.1:8899".to_string()
}

pub fn execute(address: &String) {
    let pubkey = match Pubkey::from_str(address) {
        Ok(pk) => pk,
        Err(_) => {
            eprintln!("❌ Invalid base58 pubkey string.");
            return;
        }
    };

    let rpc_url = get_rpc_url();
    println!("📡 Connecting to RPC: {}", rpc_url);
    let client = RpcClient::new(rpc_url);

    let account_data = match client.get_account_data(&pubkey) {
        Ok(data) => data,
        Err(err) => {
            eprintln!("❌ Failed to fetch account data: {}", err);
            return;
        }
    };

    if account_data.len() < 8 {
        eprintln!("❌ Account data too small to contain a discriminator");
        return;
    }

    let discriminator = &account_data[0..8];
    println!("✅ Fetched {} bytes of data.", account_data.len());

    let current_dir = std::env::current_dir().unwrap();
    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml. Please run from within a Naclac workspace.");
        return;
    };

    let idl_dir = workspace_root.join("target/idl");
    if !idl_dir.exists() {
        eprintln!("❌ Error: No IDLs found in target/idl. Run `naclac build` first.");
        return;
    }

    let mut matched_def = None;

    for entry in fs::read_dir(&idl_dir).unwrap() {
        if let Ok(entry) = entry {
            if let Ok(content) = fs::read_to_string(entry.path()) {
                if let Ok(idl) = serde_json::from_str::<Idl>(&content) {
                    for acc in idl.accounts {
                        let preimage = format!("account:{}", acc.name);
                        let mut hasher = Sha256::new();
                        hasher.update(preimage.as_bytes());
                        let hashed = hasher.finalize();
                        if &hashed[0..8] == discriminator {
                            matched_def = Some(acc);
                            break;
                        }
                    }
                }
            }
        }
    }

    let def = match matched_def {
        Some(d) => d,
        None => {
            eprintln!("❌ Failed to match account discriminator with any local IDL struct.");
            return;
        }
    };

    println!("🎯 Type Matched: {}", def.name);

    let mut offset = 8;
    let mut result_obj = Map::new();

    for field in def.ty.fields {
        let ty_str = if field.ty.is_string() {
            field.ty.as_str().unwrap().to_string()
        } else {
            // handle objects, arrays, etc. (simplified for now)
            "unknown".to_string()
        };

        if offset >= account_data.len() {
            break;
        }

        match ty_str.as_str() {
            "publicKey" => {
                if offset + 32 <= account_data.len() {
                    let pk_bytes = &account_data[offset..offset + 32];
                    let pk = bs58::encode(pk_bytes).into_string();
                    result_obj.insert(field.name.clone(), Value::String(pk));
                    offset += 32;
                }
            }
            "u8" | "i8" => {
                if offset + 1 <= account_data.len() {
                    let val = account_data[offset];
                    result_obj.insert(field.name.clone(), Value::Number(serde_json::Number::from(val)));
                    offset += 1;
                }
            }
            "u16" | "i16" => {
                if offset + 2 <= account_data.len() {
                    let mut arr = [0u8; 2];
                    arr.copy_from_slice(&account_data[offset..offset+2]);
                    let val = u16::from_le_bytes(arr);
                    result_obj.insert(field.name.clone(), Value::Number(serde_json::Number::from(val)));
                    offset += 2;
                }
            }
            "u32" | "i32" => {
                if offset + 4 <= account_data.len() {
                    let mut arr = [0u8; 4];
                    arr.copy_from_slice(&account_data[offset..offset+4]);
                    let val = u32::from_le_bytes(arr);
                    result_obj.insert(field.name.clone(), Value::Number(serde_json::Number::from(val)));
                    offset += 4;
                }
            }
            "u64" | "i64" => {
                if offset + 8 <= account_data.len() {
                    let mut arr = [0u8; 8];
                    arr.copy_from_slice(&account_data[offset..offset+8]);
                    let val = u64::from_le_bytes(arr);
                    result_obj.insert(field.name.clone(), Value::Number(serde_json::Number::from(val)));
                    offset += 8;
                }
            }
            "bool" => {
                if offset + 1 <= account_data.len() {
                    let val = account_data[offset] != 0;
                    result_obj.insert(field.name.clone(), Value::Bool(val));
                    offset += 1;
                }
            }
            _ => {
                result_obj.insert(field.name.clone(), Value::String(format!("unsupported type: {}", ty_str)));
                // We cannot safely advance offset for dynamic lengths without robust parsing
                break; 
            }
        }
    }

    let json_bytes = serde_json::to_string_pretty(&Value::Object(result_obj)).unwrap();
    println!("\n{}", json_bytes);
}

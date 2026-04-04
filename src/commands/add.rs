use std::fs;
use std::process::Command;
use heck::ToSnakeCase;

pub fn execute(name: &str) {
    let current_dir = std::env::current_dir().unwrap();

    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml. Are you inside a Naclac Workspace?");
        std::process::exit(1);
    };

    let snake_name = name.to_snake_case();
    let program_dir = workspace_root.join("programs").join(&snake_name);

    if program_dir.exists() {
        eprintln!("❌ Error: Program '{}' already exists at {:?}", snake_name, program_dir.strip_prefix(&workspace_root).unwrap_or(&program_dir));
        std::process::exit(1);
    }

    println!("🏗️  Initializing new Naclac program: '{}'...", snake_name);

    fs::create_dir_all(program_dir.join("src/instructions")).unwrap();
    fs::create_dir_all(program_dir.join("src/components")).unwrap();
    fs::create_dir_all(workspace_root.join("target/deploy")).unwrap();

    let keypair_path = workspace_root.join("target/deploy").join(format!("{}-keypair.json", snake_name));

    println!("🔑 Generating cryptographic root identity...");
    Command::new("solana-keygen")
        .arg("new")
        .arg("--no-bip39-passphrase")
        .arg("-o")
        .arg(&keypair_path)
        .arg("--force")
        .output()
        .expect("Failed to initialize system program keypair.");

    let pubkey_output = Command::new("solana-keygen")
        .arg("pubkey")
        .arg(&keypair_path)
        .output()
        .expect("Failed to derive valid public address from generator hash.");

    let address = String::from_utf8_lossy(&pubkey_output.stdout).trim().to_string();

    // ==================
    // 🛠️ WRITING FILES
    // ==================

    fs::create_dir_all(program_dir.join("src/systems")).unwrap();

    let cargo_toml = format!(r#"[package]
name = "{}"
version = "0.1.0"
description = "Created with Naclac Framework"
edition = "2021"

[lib]
crate-type = ["cdylib", "lib"]

[features]
no-entrypoint = []
no-idl = []
no-log-ix-name = []
cpi = ["no-entrypoint"]
default = []

[dependencies]
naclac-lang = "1.1.0"
"#, snake_name);

    let lib_rs = format!(r#"use naclac_lang::prelude::*;

declare_id!("{}");

pub mod components;
pub mod instructions;
pub mod systems;
pub mod events;
pub mod errors;

#[naclac_program]
pub mod {} {{
    pub fn initialize(program_id: &Pubkey, accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {{
        crate::instructions::initialize::initialize(program_id, accounts, instruction_data)
    }}

    pub fn increment(program_id: &Pubkey, accounts: &[AccountInfo], instruction_data: &[u8]) -> ProgramResult {{
        crate::instructions::increment::increment(program_id, accounts, instruction_data)
    }}
}}
"#, address, snake_name);

    fs::write(program_dir.join("Cargo.toml"), cargo_toml).unwrap();
    fs::write(program_dir.join("src/lib.rs"), lib_rs).unwrap();

    fs::write(
        program_dir.join("src/components/mod.rs"),
        "pub mod counter;\n",
    )
    .unwrap();
    let counter_rs = r#"use naclac_lang::prelude::*;

#[component]
pub struct Counter {
    pub count: u64,
    pub authority: Pubkey,
}
"#;
    fs::write(program_dir.join("src/components/counter.rs"), counter_rs).unwrap();

    fs::write(program_dir.join("src/systems/mod.rs"), "pub mod math;\n").unwrap();
    let math_rs = r#"use naclac_lang::prelude::*;
use crate::components::counter::Counter;
use crate::errors::CounterError;

#[system]
pub fn process_increment(counter: &mut Counter) -> Result<u64, ProgramError> {
    if counter.count >= 5 {
        return Err(CounterError::MaxLimitReached.into());
    }
    counter.count += 1;
    Ok(counter.count)
}
"#;
    fs::write(program_dir.join("src/systems/math.rs"), math_rs).unwrap();

    fs::write(
        program_dir.join("src/instructions/mod.rs"),
        "pub mod initialize;\npub mod increment;\n",
    )
    .unwrap();
    let initialize_rs = r#"use naclac_lang::prelude::*;
use crate::components::counter::Counter;

#[instruction]
pub fn initialize(
    #[signer]
    #[writable]
    payer: &AccountInfo,
    
    #[pda([b"counter_v2"])]
    #[init(payer = "payer", component = "Counter")] 
    #[writable]
    counter_account: &AccountInfo,
    
    system_program: &AccountInfo,
) {
    let mut data = counter_account.try_borrow_mut_data()?;
    let counter_struct = Counter::load_mut(&mut data)?;

    counter_struct.authority = *payer.key;
    counter_struct.count = 0;

    Ok(())
}
"#;
    fs::write(program_dir.join("src/instructions/initialize.rs"), initialize_rs).unwrap();

    let increment_rs = r#"use naclac_lang::prelude::*;
use crate::components::counter::Counter;
use crate::systems::math;
use crate::events::CounterIncremented;
use crate::errors::CounterError;

#[instruction]
pub fn increment(
    bump: u8, 
    #[signer] 
    #[writable]
    authority: &AccountInfo, 
    
    #[pda([b"counter_v2"], bump)]
    #[writable] 
    counter_account: &mut Counter,
) {
    if counter_account.authority != *authority.key {
        return Err(CounterError::Unauthorized.into()); 
    }

    let new_count = math::process_increment(counter_account)?;

    CounterIncremented {
        new_count,
        timestamp: unix_timestamp()?, 
    }.emit();

    Ok(())
}
"#;
    fs::write(
        program_dir.join("src/instructions/increment.rs"),
        increment_rs,
    )
    .unwrap();

    fs::write(program_dir.join("src/events.rs"), r#"use naclac_lang::prelude::*;

#[event]
pub struct CounterIncremented {
    pub new_count: u64,
    pub timestamp: i64,
}
"#).unwrap();

    fs::write(program_dir.join("src/errors.rs"), r#"use naclac_lang::prelude::*;

#[error_code]
pub enum CounterError {
    /// The counter has reached its absolute maximum limit.
    MaxLimitReached, 
    
    /// User is not authorized to perform this action.
    Unauthorized,   
}
"#).unwrap();

    // ==================
    // 🔄 SYNCING NACLAC.TOML & ROOT CARGO.TOML
    // ==================
    let config_path = workspace_root.join("Naclac.toml");
    let mut config_text = fs::read_to_string(&config_path).unwrap_or_default();
    
    // Inject into Naclac.toml
    let target_section = if config_text.contains("[programs.localnet]") {
        "[programs.localnet]"
    } else if config_text.contains("[programs.devnet]") {
        "[programs.devnet]"
    } else if config_text.contains("[programs.mainnet]") {
        "[programs.mainnet]"
    } else {
        ""
    };

    if !target_section.is_empty() {
        if let Some(pos) = config_text.find(target_section) {
            let insert_idx = pos + target_section.len();
            config_text.insert_str(insert_idx, &format!("\n{} = \"{}\"", snake_name, address));
            fs::write(&config_path, config_text).unwrap();
        }
    } else {
        config_text.push_str(&format!("\n\n[programs.localnet]\n{} = \"{}\"", snake_name, address));
        fs::write(&config_path, config_text).unwrap();
    }

    let root_cargo_path = workspace_root.join("Cargo.toml");
    if root_cargo_path.exists() {
        let mut root_cargo = fs::read_to_string(&root_cargo_path).unwrap();
        if let Some(pos) = root_cargo.find("members = [") {
            let insert_idx = pos + "members = [".len();
            root_cargo.insert_str(insert_idx, &format!("\n    \"programs/{}\",", snake_name));
            fs::write(&root_cargo_path, root_cargo).unwrap();
        }
    }


    let type_name = {
        let mut c = snake_name.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    };

    let test_ts = format!(r#"import * as naclac from "@naclac/client";
import {{ {}Client, constants }} from "../clients/src/generated";

describe("Naclac {} Test", () => {{
  it("Initializes, Increments, and emits an Event", async () => {{
    const payer = await naclac.loadNodeWallet();
    const client = new {}Client("localnet", payer);

    const [pdaAddress, bump] = await naclac.getProgramDerivedAddress({{
      programAddress: constants.PROGRAM_ID,
      seeds: [new TextEncoder().encode("counter_v2")],
    }});

    let capturedEvent: any = null;
    const listenerId = client.onCounterIncremented((event) => {{
      capturedEvent = event;
      console.log(`   🔔 Event Fired! New Count: ${{event.newCount}}`);
    }});

    console.log("   🚀 Initializing Counter...");
    await client.initialize({{}}, {{ payer: payer.address }}).rpc();

    console.log("   📈 Incrementing Counter...");
    await client.increment({{ bump }}, {{ authority: payer.address }}).rpc();

    const counterState = await client.fetchCounter(pdaAddress);
    if (Number(counterState.count) !== 1) throw new Error("Count should be 1");
    if (counterState.authority.toString() !== payer.address.toString()) throw new Error("Authority mismatch");

    await new Promise((resolve) => setTimeout(resolve, 2000));
    client.removeEventListener(listenerId);
    if (!capturedEvent) throw new Error("Event was not captured");

    console.log("   ✅ Test Passed!");
  }});
}});
"#, type_name, type_name, type_name);
    fs::write(workspace_root.join(format!("tests/{}.test.ts", snake_name)), test_ts).unwrap();

    println!("✅ Securely mapped Program ID: {}", address);
    println!("🎉 '{}' is locked, loaded, and ready to disrupt!", snake_name);
}

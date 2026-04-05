use std::fs;
use std::path::Path;
use std::process::Command;

pub fn execute(name: &str) {
    let root = Path::new(name);

    if root.exists() {
        eprintln!("❌ Error: Directory '{}' already exists.", name);
        std::process::exit(1);
    }

    println!("🚀 Initializing Naclac Workspace: {}...", name);

    let program_dir = root.join(format!("programs/{}", name));
    fs::create_dir_all(program_dir.join("src/components")).unwrap();
    fs::create_dir_all(program_dir.join("src/systems")).unwrap();
    fs::create_dir_all(program_dir.join("src/instructions")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    let deploy_dir = root.join("target/deploy");
    fs::create_dir_all(&deploy_dir).unwrap();

    println!("🔑 Generating Program Keypair...");
    let keypair_path = deploy_dir.join(format!("{}-keypair.json", name));
    Command::new("solana-keygen")
        .arg("new")
        .arg("--no-bip39-passphrase")
        .arg("-o")
        .arg(&keypair_path)
        .arg("--force")
        .output()
        .expect("Failed to execute solana-keygen.");
    let pubkey_output = Command::new("solana-keygen")
        .arg("pubkey")
        .arg(&keypair_path)
        .output()
        .expect("Failed to get pubkey");
    let program_id = String::from_utf8_lossy(&pubkey_output.stdout)
        .trim()
        .to_string();
    println!("✅ Program ID generated: {}", program_id);

    let root_cargo = r#"[workspace]
members = ["programs/*"]
resolver = "2"

[profile.release]
opt-level = 3
lto = "fat"
codegen-units = 1
panic = "abort"
strip = true
overflow-checks = true
"#;
    fs::write(root.join("Cargo.toml"), root_cargo).unwrap();

    let inner_cargo = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"

[lib]
crate-type = ["cdylib", "lib"]

[features]
no-entrypoint = []
cpi = ["no-entrypoint"]
custom-heap = []
custom-panic = []
debug-mode = []

[dependencies]
naclac-lang = "1.1.0"

[lints.rust]
unexpected_cfgs = {{ level = "warn", check-cfg = ['cfg(target_os, values("solana"))'] }}
"#,
        name
    );
    fs::write(program_dir.join("Cargo.toml"), inner_cargo).unwrap();

    let lib_rs = format!(
        r#"use naclac_lang::prelude::*;

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
"#,
        program_id, name
    );
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

    let naclac_toml = format!(
        r#"[toolchain]
package_manager = "yarn"

[features]
resolution = true
skip-lint = false

[programs.localnet]
{} = "{}"

[provider]
cluster = "localnet"
wallet = "~/.config/solana/id.json"

[scripts]
test = "yarn run ts-mocha -p ./tsconfig.json -t 1000000 \"tests/**/*.ts\""
"#,
        name, program_id
    );
    fs::write(root.join("Naclac.toml"), naclac_toml).unwrap();

    let package_json = format!(
        r#"{{
  "name": "{name}",
  "version": "0.1.0",
  "description": "{name} — a Solana program built with the Naclac Framework",
  "license": "MIT",
  "scripts": {{
    "test": "ts-mocha -p ./tsconfig.json -t 1000000 \"tests/**/*.ts\"",
    "lint:fix": "prettier */*.js \"*/**/*{{.js,.ts}}\" -w",
    "lint": "prettier */*.js \"*/**/*{{.js,.ts}}\" --check"
  }},
  "dependencies": {{
    "@naclac/client": "^1.0.0"
  }},
  "devDependencies": {{
    "@types/node": "^25.5.0",
    "@types/mocha": "^9.0.0",
    "mocha": "^9.0.3",
    "prettier": "^3.0.0",
    "ts-mocha": "^11.1.0",
    "ts-node": "^10.9.2",
    "typescript": "^5.7.3"
  }}
}}
"#,
        name = name
    );
    fs::write(root.join("package.json"), package_json).unwrap();

    let type_name = {
        let mut c = name.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    };

    let test_ts = format!(r#"import * as naclac from "@naclac/client";
import {{ {}Client, constants }} from "../clients/src/generated/{}";

describe("Naclac Counter Test", () => {{
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
"#, type_name, name);

    fs::write(root.join("tsconfig.json"), r#"{
  "compilerOptions": {
    "types": ["mocha", "node"],
    "typeRoots": ["./node_modules/@types"],
    "lib": ["es2020"],
    "module": "commonjs",
    "moduleResolution": "node",
    "target": "es2020",
    "esModuleInterop": true,
    "strict": true,
    "resolveJsonModule": true,
    "outDir": "dist"
  },
  "include": ["tests/**/*.ts"]
}
"#).unwrap();
    fs::write(root.join(".gitignore"), "target/\nCargo.lock\n**/*.rs.bk\nnode_modules/\ndist/\nyarn.lock\npackage-lock.json\npnpm-lock.yaml\ntest-ledger/\n.anchor/\n*.log\n.env\nkeys/\n*.json\n!package.json\n!tsconfig.json\n").unwrap();
    fs::write(
        root.join(".prettierignore"),
        "node_modules/\ntarget/\ntest-ledger/\ndist/\n",
    )
    .unwrap();
    fs::write(
        root.join(format!("tests/{}.test.ts", name)),
        test_ts,
    )
    .unwrap();

    println!("📦 Installing Node dependencies...");
    let mut deps_installed = false;

    for pm in ["yarn", "pnpm", "npm"].iter() {
        if Command::new(pm).arg("--version").output().is_ok() {
            println!("   Using {} to install...", pm);
            
            if let Ok(status) = Command::new(pm)
                .arg("install")
                .current_dir(&root)
                .status() 
            {
                if status.success() {
                    deps_installed = true;
                    break;
                } else {
                    println!("   ⚠️ {} failed to install dependencies. Trying fallback...", pm);
                }
            }
        }
    }

    if !deps_installed {
        println!("   ❌ Warning: All package managers failed to install dependencies (check your network). You can install them manually later.");
    }
    Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()
        .ok();
    println!("\n✅ Success! Naclac Workspace '{}' generated.", name);
}

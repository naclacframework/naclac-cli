use serde::{Deserialize, Serialize};
use std::fs;

#[derive(Serialize, Deserialize, Default)]
pub struct Idl {
    pub address: String,
    pub metadata: IdlMetadata,
    pub instructions: Vec<IdlInstruction>,
    pub accounts: Vec<IdlAccountDef>,
    pub events: Vec<IdlEventDef>,
    pub errors: Vec<IdlErrorDef>,
    pub constants: Vec<IdlConstant>,
}

#[derive(Serialize, Deserialize, Default)]
pub struct IdlMetadata {
    pub name: String,
    pub version: String,
    pub description: String,
}

#[derive(Serialize, Deserialize)]
pub struct IdlInstruction {
    pub name: String,
    pub discriminator: [u8; 8],
    pub accounts: Vec<IdlAccount>,
    pub args: Vec<IdlField>,
}

#[derive(Serialize, Deserialize)]
pub struct IdlAccount {
    pub name: String,
    #[serde(rename = "isMut")]
    pub is_mut: bool,
    #[serde(rename = "isSigner")]
    pub is_signer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pda: Option<IdlPda>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct IdlPda {
    pub seeds: Vec<IdlSeed>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind")]
pub enum IdlSeed {
    #[serde(rename = "const")]
    Const { value: Vec<u8> },
    #[serde(rename = "arg")]
    Arg { path: String },
    #[serde(rename = "account")]
    Account { path: String },
}

#[derive(Serialize, Deserialize)]
pub struct IdlAccountDef {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: IdlTypeDef,
}

#[derive(Serialize, Deserialize)]
pub struct IdlTypeDef {
    pub kind: String,
    pub fields: Vec<IdlField>,
}

#[derive(Serialize, Deserialize)]
pub struct IdlField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

#[derive(Serialize, Deserialize)]
pub struct IdlEventDef {
    pub name: String,
    pub fields: Vec<IdlEventField>,
}

#[derive(Serialize, Deserialize)]
pub struct IdlEventField {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub index: bool,
}

#[derive(Serialize, Deserialize)]
pub struct IdlErrorDef {
    pub code: u32,
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub msg: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct IdlConstant {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
    pub value: String,
}

// Map IDL type to TS type
fn map_type_to_ts(idl_type: &str) -> String {
    let clean_type = idl_type.replace(" ", "");
    if clean_type.starts_with("Option<") && clean_type.ends_with(">") {
        let inner = &clean_type[7..clean_type.len() - 1];
        return format!("{} | null", map_type_to_ts(inner));
    }
    if clean_type.starts_with("Vec<") && clean_type.ends_with(">") {
        let inner = &clean_type[4..clean_type.len() - 1];
        return format!("Array<{}>", map_type_to_ts(inner));
    }

    match clean_type.as_str() {
        "u8" | "u16" | "u32" | "i8" | "i16" | "i32" | "f32" | "f64" => "number".to_string(),
        "u64" | "u128" | "i64" | "i128" => "bigint | number".to_string(),
        "bool" => "boolean".to_string(),
        "string" | "String" => "string".to_string(),
        "publicKey" | "Pubkey" => "naclac.Address | string".to_string(),
        "bytes" => "Uint8Array".to_string(),
        _ => clean_type.to_string(), // Probably a custom type
    }
}

pub fn execute(program_id: Option<&str>) {
    // 1. Resolve workspace root
    let current_dir = std::env::current_dir().unwrap();
    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!("❌ Error: Could not find Naclac.toml.");
        std::process::exit(1);
    };

    let target_idl_dir = workspace_root.join("target/idl");
    if !target_idl_dir.exists() {
        eprintln!("❌ Error: target/idl does not exist. Run naclac build first.");
        std::process::exit(1);
    }

    let mut target_json_paths = Vec::new();

    if let Some(pid) = program_id {
        for entry in fs::read_dir(&target_idl_dir).unwrap() {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().unwrap_or_default() == "json" {
                    let content = fs::read_to_string(&path).unwrap();
                    let idl: Idl = serde_json::from_str(&content).unwrap();
                    if &idl.address == pid || &idl.metadata.name == pid {
                        target_json_paths.push((path, idl.metadata.name.clone()));
                        break;
                    }
                }
            }
        }
    } else {
        for entry in fs::read_dir(&target_idl_dir).unwrap() {
            if let Ok(entry) = entry {
                let path = entry.path();
                if path.extension().unwrap_or_default() == "json" {
                    let content = fs::read_to_string(&path).unwrap();
                    let idl: Idl = serde_json::from_str(&content).unwrap();
                    target_json_paths.push((path, idl.metadata.name.clone()));
                }
            }
        }
    }

    if target_json_paths.is_empty() {
        eprintln!("❌ Error: No IDL found. Run naclac build first.");
        std::process::exit(1);
    }

    for (json_path, program_name) in target_json_paths {
        println!("🛠 Generating TypeScript Client SDK for '{}'...", program_name);

        let content = fs::read_to_string(&json_path).unwrap();
        let idl: Idl = serde_json::from_str(&content).unwrap();

        let clients_dir = workspace_root.join(format!("clients/src/generated/{}", program_name));
        if clients_dir.exists() {
            fs::remove_dir_all(&clients_dir).unwrap();
        }
        fs::create_dir_all(&clients_dir.join("instructions")).unwrap();
        fs::create_dir_all(&clients_dir.join("accounts")).unwrap();
        fs::create_dir_all(&clients_dir.join("types")).unwrap();
        fs::create_dir_all(&clients_dir.join("idl")).unwrap();

        let target_types_dir = workspace_root.join("target/types");
        let ts_idl_path = target_types_dir.join(format!("{}.ts", program_name));
        if ts_idl_path.exists() {
            fs::copy(&ts_idl_path, clients_dir.join(format!("idl/{}.ts", program_name))).unwrap();
        }
        let json_idl_path = workspace_root.join(format!("target/idl/{}.json", program_name));
        if json_idl_path.exists() {
            fs::copy(&json_idl_path, clients_dir.join(format!("idl/{}.json", program_name))).unwrap();
        }

    let header = "// 🛑 DO NOT EDIT - AUTO-GENERATED\n\n";

    // programId replaced by constants.ts

    // types/accounts.ts
    let mut accounts_content = header.to_string();
    accounts_content.push_str("import * as naclac from \"@naclac/client\";\n\n");
    for type_def in &idl.accounts {
        accounts_content.push_str(&format!("export interface {} {{\n", type_def.name));
        for field in &type_def.ty.fields {
            accounts_content.push_str(&format!("  {}: {};\n", field.name, map_type_to_ts(&field.ty)));
        }
        accounts_content.push_str("}\n\n");
    }
    fs::write(clients_dir.join("types/accounts.ts"), accounts_content).unwrap();

    // types/events.ts
    let mut events_content = header.to_string();
    events_content.push_str("import * as naclac from \"@naclac/client\";\n\n");
    if !idl.events.is_empty() {
        for event_def in &idl.events {
            events_content.push_str(&format!("export interface {} {{\n", event_def.name));
            for field in &event_def.fields {
                events_content.push_str(&format!("  {}: {};\n", field.name, map_type_to_ts(&field.ty)));
            }
            events_content.push_str("}\n\n");
            
            events_content.push_str(&format!("export function add{}Listener(\n", event_def.name));
            events_content.push_str("  program: naclac.Program,\n");
            events_content.push_str(&format!("  callback: (event: {}, slot: number, signature: string) => void\n", event_def.name));
            events_content.push_str("): number {\n");
            events_content.push_str(&format!("  return program.addEventListener(\"{}\", callback);\n", event_def.name));
            events_content.push_str("}\n\n");
        }
        events_content.push_str("export function removeListener(program: naclac.Program, listenerId: number) {\n");
        events_content.push_str("  program.removeEventListener(listenerId);\n");
        events_content.push_str("}\n");
    }
    fs::write(clients_dir.join("types/events.ts"), events_content).unwrap();

    // types/constants.ts
    let mut constants_content = header.to_string();
    constants_content.push_str("import * as naclac from \"@naclac/client\";\n\n");
    constants_content.push_str(&format!("export const PROGRAM_ID = naclac.address(\"{}\");\n", idl.address));
    for constant in &idl.constants {
        constants_content.push_str(&format!("export const {}: {} = {};\n", constant.name, map_type_to_ts(&constant.ty), constant.value));
    }
    fs::write(clients_dir.join("types/constants.ts"), constants_content).unwrap();

    // types/errors.ts
    let mut errors_content = header.to_string();
    errors_content.push_str("export const ProgramErrors = {\n");
    for err in &idl.errors {
        let msg = err.msg.clone().unwrap_or_else(|| "".to_string());
        errors_content.push_str(&format!("  {}: {{ name: \"{}\", msg: \"{}\" }},\n", err.code, err.name, msg));
    }
    errors_content.push_str("} as const;\n");
    fs::write(clients_dir.join("types/errors.ts"), errors_content).unwrap();

    // types/index.ts
    let mut types_index = header.to_string();
    types_index.push_str("export * from \"./accounts\";\n");
    if !idl.events.is_empty() {
        types_index.push_str("export * from \"./events\";\n");
    }
    types_index.push_str("export * from \"./constants\";\n");
    types_index.push_str("export * from \"./errors\";\n");
    fs::write(clients_dir.join("types/index.ts"), types_index).unwrap();

    // 5. Generate Instructions
    let mut instructions_index = header.to_string();
    for ix in &idl.instructions {
        let ix_name = &ix.name;
        instructions_index.push_str(&format!("export * from \"./{}\";\n", ix_name));

        let mut ix_content = header.to_string();
        ix_content.push_str("import * as naclac from \"@naclac/client\";\n");
        // Convert PascalCase IDL name to whatever the build generated in `target/types/...`
        // `target/types` uses PascalCase for the IDL const export.
        
        ix_content.push_str(&format!("export function {}(\n", ix_name));
        ix_content.push_str("  program: naclac.Program,\n");
        
        // Args
        ix_content.push_str("  args: {\n");
        for arg in &ix.args {
            ix_content.push_str(&format!("    {}: {};\n", arg.name, map_type_to_ts(&arg.ty)));
        }
        ix_content.push_str("  },\n");

        // Accounts (Filter)
        ix_content.push_str("  accounts: {\n");
        for acc in &ix.accounts {
            let name_lower = acc.name.to_lowercase();
            let is_system = ["systemprogram", "tokenprogram", "token2022program", "ataprogram", "rent", "sysvarrent"]
                .contains(&name_lower.as_str());
            let is_pda = acc.pda.is_some();
            
            if !is_system && !is_pda {
                ix_content.push_str(&format!("    {}: naclac.Address | string;\n", acc.name));
            } else {
                ix_content.push_str(&format!("    {}?: naclac.Address | string;\n", acc.name)); // Make it optional since builder resolves it
            }
        }
        ix_content.push_str("  }\n");
        ix_content.push_str(") {\n");
        ix_content.push_str(&format!("  return program.methods\n    .{}(args)\n    .accounts(accounts);\n", ix_name));
        ix_content.push_str("}\n");

        fs::write(clients_dir.join(format!("instructions/{}.ts", ix_name)), ix_content).unwrap();
    }
    fs::write(clients_dir.join("instructions/index.ts"), instructions_index).unwrap();

    // 6. Generate Account Fetchers
    let mut accounts_index = header.to_string();
    for acct in &idl.accounts {
        let acct_name = &acct.name; // e.g. "Counter"
        let file_name = acct_name.to_lowercase(); // e.g. "counter"
        accounts_index.push_str(&format!("export * as {} from \"./{}\";\n", file_name, file_name));

        let mut acct_content = header.to_string();
        acct_content.push_str("import * as naclac from \"@naclac/client\";\n");
        acct_content.push_str(&format!("import type {{ {} }} from \"../types\";\n\n", acct_name));

        acct_content.push_str(&format!("export async function fetch(program: naclac.Program, address: naclac.Address | string): Promise<{}> {{\n", acct_name));
        acct_content.push_str(&format!("  const result = await program.account.{}.fetch(address);\n", acct_name));
        acct_content.push_str(&format!("  return result as unknown as {};\n", acct_name));
        acct_content.push_str("}\n\n");

        acct_content.push_str(&format!("export async function fetchAll(program: naclac.Program): Promise<Array<{{ publicKey: naclac.Address; account: {} }}>> {{\n", acct_name));
        acct_content.push_str(&format!("  const results = await program.account.{}.all();\n", acct_name));
        acct_content.push_str(&format!("  return results as unknown as Array<{{ publicKey: naclac.Address; account: {} }}>;\n", acct_name));
        acct_content.push_str("}\n");

        fs::write(clients_dir.join(format!("accounts/{}.ts", file_name)), acct_content).unwrap();
    }
    fs::write(clients_dir.join("accounts/index.ts"), accounts_index).unwrap();

    let type_name = {
        let mut c = program_name.chars();
        match c.next() {
            None => String::new(),
            Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        }
    };

    // 7. Generate Unified Client Wrapper
    let mut client_content = header.to_string();
    client_content.push_str("import * as naclac from \"@naclac/client\";\n");
    client_content.push_str(&format!("// eslint-disable-next-line @typescript-eslint/no-var-requires\nconst IDL = require(\"./idl/{}.json\"); // JSON for reliable runtime loading\n", program_name));
    
    if !idl.instructions.is_empty() {
        client_content.push_str("import * as instructions from \"./instructions\";\n");
    }
    if !idl.accounts.is_empty() {
        client_content.push_str("import * as accounts from \"./accounts\";\n");
    }
    client_content.push_str("import * as types from \"./types\";\n");
    
    client_content.push_str(&format!("\nexport class {}Client {{\n", type_name));
    client_content.push_str("  public program: naclac.Program;\n");
    if !idl.instructions.is_empty() {
        client_content.push_str("  public instructions = instructions;\n");
    }
    if !idl.accounts.is_empty() {
        client_content.push_str("  public accounts = accounts;\n");
    }
    client_content.push_str("  public types = types;\n");
    client_content.push_str("  public errors = types.ProgramErrors;\n");

    client_content.push_str("\n  constructor(providerOrCluster: naclac.NaclacProvider | \"devnet\" | \"mainnet\" | \"localnet\", payer?: naclac.KeyPairSigner) {\n");
    client_content.push_str("    let provider: naclac.NaclacProvider;\n");
    client_content.push_str("    if (typeof providerOrCluster === \"string\") {\n");
    client_content.push_str("      if (!payer) throw new Error(\"Payer is required when specifying a cluster string.\");\n");
    client_content.push_str("      provider = naclac.createProvider(providerOrCluster, payer);\n");
    client_content.push_str("    } else {\n");
    client_content.push_str("      provider = providerOrCluster;\n");
    client_content.push_str("    }\n");
    client_content.push_str("    this.program = new naclac.Program(IDL, provider);\n");
    client_content.push_str("  }\n\n");

    // Flatten instructions
    for ix in &idl.instructions {
        client_content.push_str(&format!("  public {}(args: {{\n", ix.name));
        for arg in &ix.args {
            client_content.push_str(&format!("    {}: {};\n", arg.name, map_type_to_ts(&arg.ty)));
        }
        client_content.push_str("  }, accounts: {\n");
        for acc in &ix.accounts {
            let name_lower = acc.name.to_lowercase();
            let is_system = ["systemprogram", "tokenprogram", "token2022program", "ataprogram", "rent", "sysvarrent"]
                .contains(&name_lower.as_str());
            let is_pda = acc.pda.is_some();
            if !is_system && !is_pda {
                client_content.push_str(&format!("    {}: naclac.Address | string;\n", acc.name));
            } else {
                client_content.push_str(&format!("    {}?: naclac.Address | string;\n", acc.name));
            }
        }
        client_content.push_str("  }) {\n");
        client_content.push_str(&format!("    return instructions.{}(this.program, args, accounts);\n", ix.name));
        client_content.push_str("  }\n\n");
    }

    // Flatten fetchers
    for acct in &idl.accounts {
        let file_name = acct.name.to_lowercase();
        client_content.push_str(&format!("  public fetch{}(address: naclac.Address | string) {{\n", acct.name));
        client_content.push_str(&format!("    return accounts.{}.fetch(this.program, address);\n", file_name));
        client_content.push_str("  }\n\n");
    }

    // Flatten events
    for event in &idl.events {
        client_content.push_str(&format!("  public on{}(callback: (event: types.{}, slot: number, signature: string) => void) {{\n", event.name, event.name));
        client_content.push_str(&format!("    return types.add{}Listener(this.program, callback);\n", event.name));
        client_content.push_str("  }\n\n");
    }
    if !idl.events.is_empty() {
        client_content.push_str("  public removeEventListener(listenerId: number) {\n");
        client_content.push_str("    return types.removeListener(this.program, listenerId);\n");
        client_content.push_str("  }\n");
    }

    client_content.push_str("}\n");

    fs::write(clients_dir.join("client.ts"), client_content).unwrap();

    // 8. Conditional Generation
    let mut barrel = header.to_string();
    barrel.push_str(&format!("export {{ IDL, type {} }} from \"./idl/{}\";\n", type_name, program_name));
    barrel.push_str("export * as types from \"./types\";\n");
    if !idl.instructions.is_empty() {
        barrel.push_str("export * as instructions from \"./instructions\";\n");
    }
    if !idl.accounts.is_empty() {
        barrel.push_str("export * as accounts from \"./accounts\";\n");
    }
    
    barrel.push_str("export * from \"./client\";\n");

    fs::write(clients_dir.join("index.ts"), barrel).unwrap();

    println!("✅ Client SDK successfully generated in clients/src/generated/{}", program_name);
    }
}

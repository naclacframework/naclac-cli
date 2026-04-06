use heck::ToLowerCamelCase;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

// --- Anchor-Parity IDL Structs ---
#[derive(Serialize, Deserialize, Default)]
struct Idl {
    address: String,
    metadata: IdlMetadata,
    instructions: Vec<IdlInstruction>,
    accounts: Vec<IdlAccountDef>,
    events: Vec<IdlEventDef>,
    errors: Vec<IdlErrorDef>,
    constants: Vec<IdlConstant>,
}

#[derive(Serialize, Deserialize, Default)]
struct IdlMetadata {
    name: String,
    version: String,
    description: String,
}

#[derive(Serialize, Deserialize)]
struct IdlInstruction {
    name: String,
    discriminator: [u8; 8],
    accounts: Vec<IdlAccount>,
    args: Vec<IdlField>,
}

#[derive(Serialize, Deserialize)]
struct IdlAccount {
    name: String,
    #[serde(rename = "isMut")]
    is_mut: bool,
    #[serde(rename = "isSigner")]
    is_signer: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pda: Option<IdlPda>,
}

#[derive(Serialize, Deserialize, Clone)]
struct IdlPda {
    seeds: Vec<IdlSeed>,
}

#[derive(Serialize, Deserialize, Clone)]
#[serde(tag = "kind")]
enum IdlSeed {
    #[serde(rename = "const")]
    Const { value: Vec<u8> },
    #[serde(rename = "arg")]
    Arg { path: String },
    #[serde(rename = "account")]
    Account { path: String },
}

#[derive(Serialize, Deserialize)]
struct IdlAccountDef {
    name: String,
    #[serde(rename = "type")]
    ty: IdlTypeDef,
}

#[derive(Serialize, Deserialize)]
struct IdlTypeDef {
    kind: String,
    fields: Vec<IdlField>,
}

#[derive(Serialize, Deserialize)]
struct IdlField {
    name: String,
    #[serde(rename = "type")]
    ty: String,
}

// 🌟 NEW: Event Definition Structs
#[derive(Serialize, Deserialize)]
struct IdlEventDef {
    name: String,
    fields: Vec<IdlEventField>,
}

#[derive(Serialize, Deserialize)]
struct IdlEventField {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    index: bool,
}

// 🌟 NEW: Error Definition Structs
#[derive(Serialize, Deserialize)]
struct IdlErrorDef {
    code: u32,
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    msg: Option<String>,
}

// 🌟 NEW: Constant Definition Structs
#[derive(Serialize, Deserialize)]
struct IdlConstant {
    name: String,
    #[serde(rename = "type")]
    ty: String,
    value: String,
}

pub fn execute(program_id: Option<&str>, generate_client: bool) {
    let current_dir = std::env::current_dir().unwrap();

    let workspace_root = if current_dir.join("Naclac.toml").exists() {
        current_dir.clone()
    } else if current_dir.join("../../Naclac.toml").exists() {
        current_dir.join("../..").canonicalize().unwrap()
    } else {
        eprintln!(
            "❌ Error: Could not find Naclac.toml. Please run from within a Naclac workspace."
        );
        std::process::exit(1);
    };

    let programs_dir = workspace_root.join("programs");
    let target_deploy_dir = workspace_root.join("target/deploy");
    fs::create_dir_all(&target_deploy_dir).unwrap();

    let program_dirs: Vec<PathBuf> = if programs_dir.exists() {
        fs::read_dir(&programs_dir)
            .unwrap()
            .filter_map(|entry| {
                let path = entry.unwrap().path();
                let is_target = program_id.map_or(true, |tgt| path.file_name().unwrap() == tgt);
                if path.is_dir() && path.join("src/lib.rs").exists() && is_target {
                    Some(path)
                } else {
                    None
                }
            })
            .collect()
    } else {
        eprintln!("❌ Error: No valid Naclac programs found in workspace.");
        std::process::exit(1);
    };

    println!("🔍 Checking Program ID sync status...");
    for program_dir in &program_dirs {
        let program_name = program_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        let keypair_path = target_deploy_dir.join(format!("{}-keypair.json", program_name));

        if !keypair_path.exists() {
            println!(
                "   🔑 Keypair missing for '{}'. Auto-generating...",
                program_name
            );
            Command::new("solana-keygen")
                .arg("new")
                .arg("--no-bip39-passphrase")
                .arg("-o")
                .arg(&keypair_path)
                .arg("--force")
                .output()
                .expect("Failed to generate keypair");
        }

        let pubkey_output = Command::new("solana-keygen")
            .arg("pubkey")
            .arg(&keypair_path)
            .output()
            .expect("Failed to get pubkey");
        let actual_pubkey = String::from_utf8_lossy(&pubkey_output.stdout)
            .trim()
            .to_string();

        let lib_path = program_dir.join("src/lib.rs");
        if lib_path.exists() {
            let mut lib_code = fs::read_to_string(&lib_path).unwrap();
            if let Some(start) = lib_code.find("declare_id!(\"") {
                let addr_start = start + 13;
                if let Some(end_offset) = lib_code[addr_start..].find("\")") {
                    let current_pubkey = &lib_code[addr_start..addr_start + end_offset];
                    if current_pubkey != actual_pubkey {
                        println!(
                            "   🔄 Auto-Syncing lib.rs for '{}' to {}",
                            program_name, actual_pubkey
                        );
                        lib_code.replace_range(addr_start..addr_start + end_offset, &actual_pubkey);
                        fs::write(&lib_path, lib_code).unwrap();
                    }
                }
            }
        }

        let toml_path = workspace_root.join("Naclac.toml");
        if toml_path.exists() {
            let mut toml_code = fs::read_to_string(&toml_path).unwrap();
            let search_str = format!("{} = \"", program_name);
            if let Some(start) = toml_code.find(&search_str) {
                let addr_start = start + search_str.len();
                if let Some(end_offset) = toml_code[addr_start..].find("\"") {
                    let current_pubkey = &toml_code[addr_start..addr_start + end_offset];
                    if current_pubkey != actual_pubkey {
                        println!("   🔄 Auto-Syncing Naclac.toml for '{}'...", program_name);
                        toml_code
                            .replace_range(addr_start..addr_start + end_offset, &actual_pubkey);
                        fs::write(&toml_path, toml_code).unwrap();
                    }
                }
            }
        }
    }

    println!("🔨 Compiling Native SBF...");
    let mut cmd = Command::new("cargo");
    cmd.arg("build-sbf").current_dir(&workspace_root);

    if let Some(tgt) = program_id {
        cmd.arg("--manifest-path").arg(format!("programs/{}/Cargo.toml", tgt));
    }

    let mut child = cmd
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to execute cargo build-sbf");

    let build_status = child.wait().expect("Failed to wait on cargo build-sbf");

    if !build_status.success() {
        eprintln!("❌ SBF Compilation failed.");
        std::process::exit(1);
    }

    for program_dir in &program_dirs {
        let program_name = program_dir
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        println!("📄 Generating Naclac IDL & Types for '{}'...", program_name);

        let keypair_path = target_deploy_dir.join(format!("{}-keypair.json", program_name));
        let pubkey_output = Command::new("solana-keygen")
            .arg("pubkey")
            .arg(&keypair_path)
            .output()
            .unwrap();
        let actual_pubkey = String::from_utf8_lossy(&pubkey_output.stdout)
            .trim()
            .to_string();

        let mut idl = Idl {
            address: actual_pubkey,
            metadata: IdlMetadata {
                name: program_name.clone(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                description: format!("{} — Generated by Naclac Framework", program_name),
            },
            instructions: Vec::new(),
            accounts: Vec::new(),
            events: Vec::new(),    // 🌟 Init Events
            errors: Vec::new(),    // 🌟 Init Errors
            constants: Vec::new(), // 🌟 Init Constants
        };

        fn rust_type_to_idl(ty: &str) -> String {
            let cleaned = ty.replace(" ", "").replace("\n", "");
            match cleaned.as_str() {
                "Pubkey" | "solana_program::pubkey::Pubkey" => "publicKey".to_string(),
                "u8" | "u16" | "u32" | "u64" | "u128" => cleaned,
                "i8" | "i16" | "i32" | "i64" | "i128" => cleaned,
                "f32" | "f64" => cleaned,
                "bool" => "bool".to_string(),
                "String" | "&str" => "string".to_string(),
                t if t.starts_with("[u8;") => "bytes".to_string(),
                t => t.to_string(), // fallback for custom component names
            }
        }

        // ==========================================
        // 1. AST PARSER: COMPONENTS
        // ==========================================
        let components_dir = program_dir.join("src/components");
        if components_dir.exists() {
            for entry in fs::read_dir(components_dir).unwrap() {
                let path = entry.unwrap().path();
                if path.extension().unwrap_or_default() == "rs"
                    && path.file_name().unwrap() != "mod.rs"
                {
                    let code = fs::read_to_string(&path).unwrap();
                    let syntax_tree = syn::parse_file(&code).unwrap();

                    for item in syntax_tree.items {
                        if let syn::Item::Struct(item_struct) = item {
                            // 🌟 FIXED: Only parse structs that have the #[component] attribute
                            let is_component = item_struct.attrs.iter().any(|a| {
                                a.path()
                                    .segments
                                    .last()
                                    .map_or(false, |s| s.ident == "component")
                            });

                            if !is_component {
                                continue;
                            }

                            let mut fields = Vec::new();
                            for field in item_struct.fields {
                                let field_name = field
                                    .ident
                                    .as_ref()
                                    .unwrap()
                                    .to_string()
                                    .to_lower_camel_case();
                                let field_ty = &field.ty;
                                let raw_type = quote::quote!(#field_ty).to_string();
                                let field_type = rust_type_to_idl(&raw_type);
                                fields.push(IdlField {
                                    name: field_name,
                                    ty: field_type,
                                });
                            }
                            idl.accounts.push(IdlAccountDef {
                                name: item_struct.ident.to_string(),
                                ty: IdlTypeDef {
                                    kind: "struct".to_string(),
                                    fields,
                                },
                            });
                        }
                    }
                }
            }
        }

        // ==========================================
        // 2. AST PARSER: EVENTS (🌟 NEW)
        // ==========================================
        let mut parse_events = |code: &str| {
            if let Ok(syntax_tree) = syn::parse_file(code) {
                for item in syntax_tree.items {
                    if let syn::Item::Struct(item_struct) = item {
                        let is_event = item_struct.attrs.iter().any(|a| a.path().is_ident("event"));
                        if is_event {
                            let mut fields = Vec::new();
                            for field in item_struct.fields {
                                let field_name = field
                                    .ident
                                    .as_ref()
                                    .unwrap()
                                    .to_string()
                                    .to_lower_camel_case();
                                let field_ty = &field.ty;
                                let type_str =
                                    rust_type_to_idl(&quote::quote!(#field_ty).to_string());

                                fields.push(IdlEventField {
                                    name: field_name,
                                    ty: type_str,
                                    index: false,
                                });
                            }
                            idl.events.push(IdlEventDef {
                                name: item_struct.ident.to_string(),
                                fields,
                            });
                        }
                    }
                }
            }
        };

        let events_file = program_dir.join("src/events.rs");
        if events_file.exists() {
            parse_events(&fs::read_to_string(&events_file).unwrap());
        }

        // ==========================================
        // 3. AST PARSER: ERRORS (🌟 NEW)
        // ==========================================
        let mut parse_errors = |code: &str| {
            if let Ok(syntax_tree) = syn::parse_file(code) {
                for item in syntax_tree.items {
                    if let syn::Item::Enum(item_enum) = item {
                        let is_error = item_enum
                            .attrs
                            .iter()
                            .any(|a| a.path().is_ident("error_code"));
                        if is_error {
                            let mut code_offset = 6000; // Anchor standard custom error starting index
                            for variant in item_enum.variants {
                                let name = variant.ident.to_string();

                                // Safely extract /// doc comments
                                let mut msg = None;
                                for attr in variant.attrs {
                                    if attr.path().is_ident("doc") {
                                        let meta_str = quote::quote!(#attr).to_string();
                                        if let Some(start) = meta_str.find("\"") {
                                            if let Some(end) = meta_str.rfind("\"") {
                                                if start != end {
                                                    let doc_str =
                                                        meta_str[start + 1..end].trim().to_string();
                                                    msg = Some(if let Some(existing) = msg {
                                                        format!("{} {}", existing, doc_str)
                                                    } else {
                                                        doc_str
                                                    });
                                                }
                                            }
                                        }
                                    }
                                }

                                idl.errors.push(IdlErrorDef {
                                    code: code_offset,
                                    name,
                                    msg,
                                });
                                code_offset += 1;
                            }
                        }
                    }
                }
            }
        };

        let errors_file = program_dir.join("src/errors.rs");
        if errors_file.exists() {
            parse_errors(&fs::read_to_string(&errors_file).unwrap());
        }

        // ==========================================
        // 3.5 AST PARSER: CONSTANTS (🌟 NEW)
        // ==========================================
        let mut parse_constants = |code: &str| {
            if let Ok(syntax_tree) = syn::parse_file(code) {
                for item in syntax_tree.items {
                    if let syn::Item::Const(item_const) = item {
                        let is_exported = item_const
                            .attrs
                            .iter()
                            .any(|a| a.path().is_ident("constant"));

                        if is_exported {
                            let const_name = item_const.ident.to_string();

                            let ty_node = &*item_const.ty;
                            let raw_ty = quote::quote!(#ty_node).to_string();
                            let const_ty = rust_type_to_idl(&raw_ty);

                            let expr_node = &*item_const.expr;
                            let const_value = quote::quote!(#expr_node).to_string();

                            idl.constants.push(IdlConstant {
                                name: const_name,
                                ty: const_ty,
                                value: const_value,
                            });
                        }
                    }
                }
            }
        };

        let mut rs_files = Vec::new();
        let mut dirs_to_visit = vec![program_dir.join("src")];

        while let Some(dir) = dirs_to_visit.pop() {
            if let Ok(entries) = fs::read_dir(dir) {
                for entry in entries {
                    if let Ok(entry) = entry {
                        let path = entry.path();
                        if path.is_dir() {
                            dirs_to_visit.push(path);
                        } else if path.extension().unwrap_or_default() == "rs" {
                            rs_files.push(path);
                        }
                    }
                }
            }
        }

        for file_path in rs_files {
            if let Ok(code) = fs::read_to_string(&file_path) {
                parse_constants(&code);
            }
        }

        // ==========================================
        // 4. AST PARSER: INSTRUCTIONS
        // ==========================================
        let mut instruction_order = Vec::new();
        let lib_code = fs::read_to_string(program_dir.join("src/lib.rs")).unwrap();
        let lib_tree = syn::parse_file(&lib_code).unwrap();
        for item in lib_tree.items {
            if let syn::Item::Mod(item_mod) = item {
                if let Some((_, items)) = item_mod.content {
                    for inner_item in items {
                        if let syn::Item::Fn(item_fn) = inner_item {
                            instruction_order.push(item_fn.sig.ident.to_string());
                        }
                    }
                }
            }
        }

        for func_name in instruction_order {
            let mut found_path = None;
            let mut dirs_to_visit = vec![program_dir.join("src/instructions")];
            while let Some(dir) = dirs_to_visit.pop() {
                if let Ok(entries) = fs::read_dir(&dir) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            let path = entry.path();
                            if path.is_dir() {
                                dirs_to_visit.push(path);
                            } else if path.is_file() {
                                if let Some(name) = path.file_stem() {
                                    if name.to_string_lossy() == func_name {
                                        found_path = Some(path);
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                if found_path.is_some() {
                    break;
                }
            }

            if let Some(ix_path) = found_path {
                let code = fs::read_to_string(&ix_path).unwrap();
                let syntax_tree = syn::parse_file(&code).unwrap();

                for item in syntax_tree.items {
                    if let syn::Item::Fn(item_fn) = item {
                        if item_fn.sig.ident.to_string() == func_name {
                            let mut accounts = Vec::new();
                            let mut args = Vec::new();
                            let mut raw_pdas = Vec::new();
                            for arg in item_fn.sig.inputs {
                                if let syn::FnArg::Typed(pat_type) = arg {
                                    let raw_arg_name =
                                        if let syn::Pat::Ident(pat_ident) = &*pat_type.pat {
                                            pat_ident.ident.to_string()
                                        } else {
                                            continue;
                                        };

                                    let arg_name = raw_arg_name.to_lower_camel_case();

                                    let mut is_mut = false;
                                    let mut is_signer = false;
                                    for attr in &pat_type.attrs {
                                        if attr.path().is_ident("mut")
                                            || attr.path().is_ident("writable")
                                        {
                                            is_mut = true;
                                        }
                                        if attr.path().is_ident("signer") {
                                            is_signer = true;
                                        }
                                        if attr.path().is_ident("pda") {
                                            raw_pdas.push((
                                                arg_name.clone(),
                                                quote::quote!(#attr).to_string(),
                                            ));
                                        }
                                    }

                                    let ty_node = &*pat_type.ty;
                                    let type_str = quote::quote!(#ty_node).to_string();
                                    let is_account_info = type_str.contains("AccountInfo");
                                    // &[AccountInfo] is a slice — the Naclac convention for
                                    // remaining_accounts (passed as writable batch targets).
                                    let is_account_slice = type_str.contains("[")
                                        && type_str.contains("AccountInfo");
                                    let is_primitive_ref = type_str.contains("u8")
                                        || type_str.contains("u16")
                                        || type_str.contains("u32")
                                        || type_str.contains("u64")
                                        || type_str.contains("i64")
                                        || type_str.contains("str")
                                        || type_str.contains("String")
                                        || type_str.contains("bool");
                                    let is_custom_component =
                                        type_str.contains("&") && !is_primitive_ref;

                                    if is_account_info || is_custom_component {
                                        // Slice of AccountInfos = remaining_accounts; always writable.
                                        let effective_is_mut = is_mut || is_account_slice;
                                        accounts.push(IdlAccount {
                                            name: arg_name,
                                            is_mut: effective_is_mut,
                                            is_signer,
                                            pda: None,
                                        });
                                    } else {
                                        args.push(IdlField {
                                            name: arg_name,
                                            ty: rust_type_to_idl(&type_str),
                                        });
                                    }
                                }
                            }

                            // 🌟 NEW: Flawless string extraction for PDA Seeds
                            for (acc_name, attr_str) in raw_pdas {
                                if let Some(start) = attr_str.find('(') {
                                    if let Some(end) = attr_str.rfind(')') {
                                        let inside = &attr_str[start + 1..end];
                                        // Remove array brackets
                                        let clean_str = inside.replace("[", "").replace("]", "");

                                        let mut seeds = Vec::new();
                                        for part in clean_str.split(',') {
                                            let part = part
                                                .trim()
                                                .replace(".key()", "")
                                                .replace(".key", "");
                                            if part.is_empty() {
                                                continue;
                                            }

                                            if part.starts_with("b\"") && part.ends_with("\"") {
                                                // It's a byte string (Const) - perfectly matches Anchor!
                                                let val =
                                                    part[2..part.len() - 1].as_bytes().to_vec();
                                                seeds.push(IdlSeed::Const { value: val });
                                            } else {
                                                if accounts.iter().any(|a| a.name == part) {
                                                    seeds.push(IdlSeed::Account { path: part });
                                                } else {
                                                    seeds.push(IdlSeed::Arg { path: part });
                                                }
                                            }
                                        }

                                        if let Some(acc) =
                                            accounts.iter_mut().find(|a| a.name == acc_name)
                                        {
                                            acc.pda = Some(IdlPda { seeds });
                                        }
                                    }
                                }
                            }

                            let preimage = format!("global:{}", func_name);
                            let mut hasher = Sha256::new();
                            hasher.update(preimage.as_bytes());
                            let disc: [u8; 8] = hasher.finalize()[..8].try_into().unwrap();

                            idl.instructions.push(IdlInstruction {
                                name: func_name.to_lower_camel_case(),
                                discriminator: disc,
                                accounts,
                                args,
                            });
                        }
                    }
                }
            }
        }

        // rust_type_to_idl moved to the top of loop

        // ==========================================
        // FINAL IDL EXPORT
        // ==========================================
        let target_idl_dir = workspace_root.join("target/idl");
        fs::create_dir_all(&target_idl_dir).unwrap();
        let idl_json_pretty = serde_json::to_string_pretty(&idl).unwrap();
        let idl_path = target_idl_dir.join(format!("{}.json", program_name));
        fs::write(&idl_path, &idl_json_pretty).unwrap();
        println!(
            "✅ IDL written to: {:?}",
            idl_path.canonicalize().unwrap_or(idl_path)
        );

        let target_types_dir = workspace_root.join("target/types");
        fs::create_dir_all(&target_types_dir).unwrap();

        let type_name = {
            let mut c = program_name.chars();
            match c.next() {
                None => String::new(),
                Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
            }
        };

        let ts_content = format!(
            "export const IDL = {} as const;\n\nexport type {} = typeof IDL;\n",
            idl_json_pretty, type_name
        );

        let ts_path = target_types_dir.join(format!("{}.ts", program_name));
        fs::write(&ts_path, ts_content).unwrap();
        println!(
            "✅ Types written to: {:?}",
            ts_path.canonicalize().unwrap_or(ts_path)
        );
        if generate_client {
            println!("🔄 Auto-generating TypeScript SDK for '{}'...", program_name);
            crate::commands::generate::execute(Some(&program_name));
        }
    }
}

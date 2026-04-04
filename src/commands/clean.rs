use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::io::{self, Write};

pub fn execute(hard: bool) {
    let current_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));

    if hard {
        let has_node = current_dir.join("node_modules").exists();
        let has_dist = current_dir.join("dist").exists();
        
        let mut extras = String::new();
        if has_node && has_dist {
            extras = "\n   It will also wipe 'node_modules/' and 'dist/'.".to_string();
        } else if has_node {
            extras = "\n   It will also wipe 'node_modules/'.".to_string();
        } else if has_dist {
            extras = "\n   It will also wipe 'dist/'.".to_string();
        }

        println!("⚠️  WARNING: You are executing a HARD clean!");
        println!("   This will permanently delete the entire 'target/' directory,");
        println!("   including your deployed program keypairs located in 'target/deploy/'.{}", extras);
        print!("   Are you entirely sure you want to proceed? [y/N]: ");
        
        io::stdout().flush().unwrap();
        let mut input = String::new();
        io::stdin().read_line(&mut input).unwrap();
        
        if input.trim().to_lowercase() != "y" {
            println!("🛑 Clean aborted.");
            return;
        }

        println!("🧹 Sweeping entire workspace (Hard Mode)...");
        let mut cleaned = false;
        let hard_targets = ["target", "node_modules", "dist", ".anchor"];
        for target in hard_targets.iter() {
            let path = current_dir.join(target);
            if path.exists() {
                if path.is_dir() {
                    let _ = fs::remove_dir_all(&path);
                } else {
                    let _ = fs::remove_file(&path);
                }
                println!("  🗑️  Removed: {}", target);
                cleaned = true;
            }
        }
        
        if cleaned {
             println!("✨ Workspace completely wiped!");
        } else {
             println!("✨ Workspace was already empty.");
        }
    } else {
        println!("🧹 Running safe workspace clean...");
        // 1. Cargo clean under the hood
        let _ = Command::new("cargo").arg("clean").status();
        
        // Wipe target/idl safely
        let target_idl = current_dir.join("target/idl");
        if target_idl.exists() {
            let _ = fs::remove_dir_all(&target_idl);
            println!("  🗑️  Removed: target/idl");
        }
        
        // Wipe target/sbf-solana-solana (where .so files live)
        let target_sbf = current_dir.join("target/sbf-solana-solana");
        if target_sbf.exists() {
            let _ = fs::remove_dir_all(&target_sbf);
            println!("  🗑️  Removed: target/sbf-solana-solana");
        }

        let target_docs = current_dir.join("target/doc");
        if target_docs.exists() {
            let _ = fs::remove_dir_all(&target_docs);
        }

        let cache = current_dir.join(".anchor");
        if cache.exists() {
            let _ = fs::remove_dir_all(&cache);
            println!("  🗑️  Removed: .anchor");
        }

        println!("✨ Workspace cleaned securely!");
        println!("🔒 (Program Deploy keys safely preserved in target/deploy).");
        println!("💡 Run `naclac clean --hard` if you need to completely wipe everything.");
    }
}

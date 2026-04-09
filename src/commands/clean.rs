use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

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
    }

    println!("🧹 Running safe workspace clean...");

    let trash_dir = current_dir.join(".naclac_trash");
    if trash_dir.exists() {
        let _ = fs::remove_dir_all(&trash_dir);
    }
    fs::create_dir_all(&trash_dir).unwrap();

    let mut to_clean = Vec::new();

    if hard {
        to_clean.push("target");
        to_clean.push("node_modules");
        to_clean.push("dist");
        to_clean.push(".anchor");
    } else {
        // Safe clean: we want to clean target, but EXCLUDE deploy/*.json
        let target_dir = current_dir.join("target");
        if target_dir.exists() {
            let trash_target = trash_dir.join("target");
            fs::create_dir_all(&trash_target).unwrap();

            // Move everything inside target to trash/target, EXCEPT target/deploy/*.json
            if let Ok(entries) = fs::read_dir(&target_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    let name = path.file_name().unwrap();
                    let trash_path = trash_target.join(name);

                    if name == "deploy" {
                        fs::create_dir_all(&trash_path).unwrap();
                        if let Ok(deploy_entries) = fs::read_dir(&path) {
                            for d_entry in deploy_entries.flatten() {
                                let d_path = d_entry.path();
                                let mut keep = false;
                                if let Some(ext) = d_path.extension() {
                                    if ext == "json" {
                                        keep = true;
                                    }
                                }
                                if !keep {
                                    let d_name = d_path.file_name().unwrap();
                                    let _ = fs::rename(&d_path, trash_path.join(d_name));
                                }
                            }
                        }
                    } else {
                        let _ = fs::rename(&path, &trash_path);
                    }
                }
            }
        }
        to_clean.push(".anchor");
    }

    // For hard clean or .anchor, just rename them wholly
    for entry in to_clean {
        let path = current_dir.join(entry);
        if path.exists() {
            let _ = fs::rename(&path, trash_dir.join(entry));
        }
    }

    // Set up cancellation
    let cancelled = Arc::new(AtomicBool::new(false));
    let r = cancelled.clone();
    
    // ctrlc::set_handler replaces any previous handler, making sure our atomic bool updates
    ctrlc::set_handler(move || {
        r.store(true, Ordering::SeqCst);
    }).expect("Error setting Ctrl-C handler");

    // Setup Spinner
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::default_spinner()
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"])
            .template("{spinner:.green} {msg}")
            .unwrap(),
    );
    pb.set_message("Deleting files in background (Press Ctrl+C to abort and recover)...");

    // We do the deletion on a background thread.
    // To permit cancellation gracefully, we do not use fs::remove_dir_all.
    // We walk the tree and delete file-by-file so we can check `cancelled`.
    let c = cancelled.clone();
    let trash_dir_clone = trash_dir.clone();
    
    let handle = thread::spawn(move || {
        fn safe_delete_recursive(path: &Path, c: &Arc<AtomicBool>) -> bool {
            if c.load(Ordering::SeqCst) {
                return false; // Cancelled
            }
            if path.is_dir() {
                if let Ok(entries) = fs::read_dir(path) {
                    for entry in entries.flatten() {
                        if !safe_delete_recursive(&entry.path(), c) {
                            return false;
                        }
                    }
                }
                let _ = fs::remove_dir(path);
            } else {
                let _ = fs::remove_file(path);
            }
            true
        }

        let completed = safe_delete_recursive(&trash_dir_clone, &c);
        if completed && trash_dir_clone.exists() {
            let _ = fs::remove_dir_all(&trash_dir_clone);
        }
        completed
    });

    while !handle.is_finished() {
        if cancelled.load(Ordering::SeqCst) {
            pb.set_message("Cancellation requested... restoring remaining files...");
        } else {
            pb.tick();
        }
        thread::sleep(Duration::from_millis(50));
    }

    let completed = handle.join().unwrap_or(false);

    if !completed {
        // Restore from `.naclac_trash`
        if let Ok(entries) = fs::read_dir(&trash_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let name = path.file_name().unwrap();
                let original_path = current_dir.join(name);
                
                if name == "target" && !hard {
                    // Marge back into current target safely
                    if let Ok(sub_entries) = fs::read_dir(&path) {
                         for s_entry in sub_entries.flatten() {
                             let s_path = s_entry.path();
                             let s_name = s_path.file_name().unwrap();
                             let dest = original_path.join(s_name);
                             
                             if s_name == "deploy" && dest.exists() {
                                 // merge deploy
                                 if let Ok(d_entries) = fs::read_dir(&s_path) {
                                     for d in d_entries.flatten() {
                                         let _ = fs::rename(d.path(), dest.join(d.path().file_name().unwrap()));
                                     }
                                 }
                             } else {
                                 fs::create_dir_all(&original_path).unwrap();
                                 let _ = fs::rename(&s_path, dest);
                             }
                         }
                    }
                } else {
                    let _ = fs::rename(&path, &original_path);
                }
            }
        }
        let _ = fs::remove_dir_all(&trash_dir);
        pb.finish_with_message("🛑 Clean aborted entirely by user. Recoverable files were restored!");
    } else {
        if hard {
            pb.finish_with_message("✨ Workspace completely wiped!");
        } else {
            pb.finish_with_message("✨ Workspace cleaned securely!\n🔒 (Program Deploy keys safely preserved in target/deploy).\n💡 Run `naclac clean --hard` to completely wipe everything.");
        }
    }
}

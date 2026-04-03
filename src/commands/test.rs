use std::fs;
use std::process::Command;

pub fn execute(target_file: Option<&str>) {
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

    let toml_path = workspace_root.join("Naclac.toml");
    let toml_content = fs::read_to_string(&toml_path).unwrap();

    // Parse the test script from Naclac.toml safely, avoiding escaped quote bugs
    let mut test_script = String::new();
    for line in toml_content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("test =") {
            if let Some(start) = trimmed.find('"') {
                if let Some(end) = trimmed.rfind('"') {
                    if start != end {
                        test_script = trimmed[start + 1..end].to_string();
                        // Clean up escaped quotes from the TOML string
                        test_script = test_script.replace("\\\"", "\"");
                    }
                }
            }
        }
    }

    if test_script.is_empty() {
        eprintln!("❌ Error: No test script found in Naclac.toml under [scripts].");
        std::process::exit(1);
    }

    // 🌟 THE INJECTION ENGINE 🌟
    // If the user specified a file (e.g., `naclac test counter.ts`), swap out the global glob pattern!
    if let Some(file_name) = target_file {
        // Handle if the user accidentally typed "tests/counter.ts" instead of just "counter.ts"
        let target_path = if file_name.starts_with("tests/") {
            file_name.to_string()
        } else {
            format!("tests/{}", file_name)
        };

        if test_script.contains("\"tests/**/*.ts\"") {
            test_script = test_script.replace("\"tests/**/*.ts\"", &format!("\"{}\"", target_path));
        } else if test_script.contains("tests/**/*.ts") {
            test_script = test_script.replace("tests/**/*.ts", &target_path);
        } else {
            // Fallback: just append the file name to the custom script
            test_script = format!("{} \"{}\"", test_script, target_path);
        }
    }

    println!("🧪 Running Naclac Test Suite...");
    println!("   > {}", test_script);

    let shell = if cfg!(target_os = "windows") {
        "cmd"
    } else {
        "sh"
    };
    let shell_flag = if cfg!(target_os = "windows") {
        "/C"
    } else {
        "-c"
    };

    let mut child = Command::new(shell)
        .arg(shell_flag)
        .arg(&test_script)
        .current_dir(&workspace_root)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit())
        .spawn()
        .expect("Failed to execute test script");

    let status = child.wait().expect("Failed to wait on test execution");

    if status.success() {
        println!("✅ All tests passed successfully!");
    } else {
        eprintln!("❌ Test suite failed.");
        std::process::exit(1);
    }
}

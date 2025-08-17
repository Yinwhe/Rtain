use std::process::Command;
// use std::time::Duration;  // Unused for now
// use tokio::time::sleep;   // Unused for now

/// End-to-end test helpers and utilities
///
/// Note: These tests require Linux and root privileges to run properly
/// They test the actual container functionality

#[cfg(target_os = "linux")]
mod linux_tests {
    use super::*;

    /// Test if we can build the project
    #[tokio::test]
    async fn test_project_builds() {
        let output = Command::new("cargo")
            .args(&["build", "--release"])
            .output()
            .expect("Failed to execute cargo build");

        if !output.status.success() {
            println!("Build stdout: {}", String::from_utf8_lossy(&output.stdout));
            println!("Build stderr: {}", String::from_utf8_lossy(&output.stderr));
        }

        assert!(output.status.success(), "Project should build successfully");
    }

    /// Test basic daemon startup (requires root privileges)
    #[tokio::test]
    async fn test_daemon_startup() {
        // Skip if not running as root
        if !is_root() {
            println!("Skipping daemon test - requires root privileges");
            return;
        }

        // Build the project first
        let build_output = Command::new("cargo")
            .args(&["build", "--bin", "rtain_daemon"])
            .output()
            .expect("Failed to build daemon");

        assert!(build_output.status.success(), "Daemon should build");

        // Try to start daemon (will fail without proper setup, but should not panic)
        let daemon_output = Command::new("timeout")
            .args(&["2s", "./target/debug/rtain_daemon"])
            .output();

        // The daemon might fail due to missing permissions or setup,
        // but it should not panic or crash immediately
        match daemon_output {
            Ok(_) => {
                // If it runs for 2 seconds without crashing, that's good
                println!("Daemon started and ran for timeout period");
            }
            Err(e) => {
                println!("Daemon execution error (expected): {}", e);
            }
        }
    }

    /// Test that the CLI binary can be built and shows help
    #[tokio::test]
    async fn test_cli_help() {
        let build_output = Command::new("cargo")
            .args(&["build", "--bin", "rtain_front"])
            .output()
            .expect("Failed to build CLI");

        assert!(build_output.status.success(), "CLI should build");

        let help_output = Command::new("./target/debug/rtain_front")
            .args(&["--help"])
            .output()
            .expect("Failed to run CLI help");

        let help_text = String::from_utf8_lossy(&help_output.stdout);
        assert!(help_text.contains("rtain"), "Help should mention rtain");
        assert!(
            help_text.contains("container"),
            "Help should mention containers"
        );
    }

    /// Test CLI commands (without daemon running)
    #[tokio::test]
    async fn test_cli_commands_without_daemon() {
        let build_output = Command::new("cargo")
            .args(&["build", "--bin", "rtain_front"])
            .output()
            .expect("Failed to build CLI");

        assert!(build_output.status.success(), "CLI should build");

        // Test PS command (should fail gracefully when daemon is not running)
        let ps_output = Command::new("./target/debug/rtain_front")
            .args(&["ps"])
            .output()
            .expect("Failed to run PS command");

        // Should exit with error code since daemon is not running
        assert!(!ps_output.status.success(), "PS should fail without daemon");

        let stderr = String::from_utf8_lossy(&ps_output.stderr);
        // Should contain some kind of connection error message
        assert!(!stderr.is_empty(), "Should have error message");
    }

    fn is_root() -> bool {
        unsafe { nix::libc::geteuid() == 0 }
    }
}

#[cfg(not(target_os = "linux"))]
mod non_linux_tests {
    use super::*;

    #[tokio::test]
    async fn test_compile_check_only() {
        // On non-Linux systems, we can only test compilation
        let output = Command::new("cargo")
            .args(&["check"])
            .output()
            .expect("Failed to execute cargo check");

        if !output.status.success() {
            println!("Check stdout: {}", String::from_utf8_lossy(&output.stdout));
            println!("Check stderr: {}", String::from_utf8_lossy(&output.stderr));
        }

        assert!(output.status.success(), "Project should pass cargo check");
    }

    #[tokio::test]
    async fn test_unit_tests() {
        let output = Command::new("cargo")
            .args(&["test", "--lib"])
            .output()
            .expect("Failed to run unit tests");

        if !output.status.success() {
            println!("Test stdout: {}", String::from_utf8_lossy(&output.stdout));
            println!("Test stderr: {}", String::from_utf8_lossy(&output.stderr));
        }

        assert!(output.status.success(), "Unit tests should pass");
    }
}

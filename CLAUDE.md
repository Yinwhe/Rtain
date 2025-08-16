# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Rtain is a learning project implementing a lightweight container manager in Rust. It provides container lifecycle management (create, start, stop, delete) with a client-server architecture using Unix domain sockets for IPC.

## Key Commands

### Building and Development
- `cargo build` - Build the project (requires Linux environment)
- `cargo build -j 1` - Build with single thread (recommended for low-memory environments)
- `cargo check` - Check code without building (will fail on macOS due to Linux-only dependencies)
- `cargo run --bin rtain_daemon` - Run the daemon process
- `cargo run --bin rtain_front` - Run the client frontend

**Important**: This project requires Linux to compile and run due to dependencies on:
- `cgroups-rs` for container resource management
- `rtnetlink` for network management
- Linux-specific `nix` features for system calls

**macOS Development**: Use `container exec ubuntu bash` to enter the Ubuntu container for compilation and testing.

### Container Operations
The project provides these container management commands through the CLI:
- `run` - Create and start a new container
- `start` - Start an existing container
- `stop` - Stop a running container
- `rm` - Remove a container
- `ps` - List containers
- `exec` - Execute commands in a running container
- `logs` - Show container logs
- `commit` - Commit container changes to an image

### Network Operations
- `network create` - Create a new network

### Testing
- `cargo test --lib` - Run unit tests (requires Linux environment)
- `cargo test --lib -j 1` - Run tests with single thread (recommended for low-memory environments)
- `make test-unit` - Run unit tests using Makefile
- `make test` - Run all tests (unit + integration)
- `make help` - Show all available make targets

**Test Framework**: Comprehensive test suite with:
- Unit tests for all core modules (cmd, msg, network/ipam, metas/storage)
- Integration tests using `tempfile` for test isolation
- Mock testing with `tokio-test` and `mockall`
- Test coverage includes IP allocation, message serialization, and storage operations

## Architecture

### Core Components

**Client-Server Architecture**: The system is split into two main binaries:
- `rtain_daemon` (`src/bin/rtain_daemon.rs`) - Server daemon handling container operations
- `rtain_front` (`src/bin/rtain_front.rs`) - Client frontend for user interaction

**Communication**: Uses Unix domain socket at `/tmp/rtain_daemons.sock` for IPC between client and daemon.

### Module Structure

- **`src/core/`** - Core container management functionality
  - `container/` - Container lifecycle operations (start, stop, exec, etc.)
  - `network/` - Network management and bridge configuration
  - `metas/` - Metadata and storage management for containers
  - `cmd.rs` - Command definitions and parsing
  - `msg.rs` - Message protocol for client-daemon communication

- **`src/front/`** - Frontend interfaces
  - `cli.rs` - Command-line interface using clap
  - `ops.rs` - Client-side operation implementations

### Key Patterns

1. **Async Architecture**: Uses Tokio runtime for async operations
2. **Message Protocol**: Custom serialization with bincode for client-daemon communication
3. **Metadata Management**: Persistent storage of container and network metadata
4. **Resource Management**: Integration with Linux cgroups and networking primitives

### Important Paths
- Container root: `/tmp/rtain`
- Daemon socket: `/tmp/rtain_daemons.sock`
- Network config: `/tmp/rtain/net/networks`

### Dependencies

**Core Dependencies**:
- `tokio` - Async runtime with full features
- `clap` - CLI argument parsing
- `nix` - System call bindings (sched, signal, mount, fs, term features)
- `cgroups-rs` - Container resource management
- `rtnetlink` - Network configuration
- `anyhow` - Error handling
- `serde` + `bincode` - Serialization for IPC
- `dashmap` - Concurrent hashmap for metadata storage

**Testing Dependencies**:
- `tokio-test` - Async testing utilities
- `tempfile` - Temporary file and directory management for test isolation
- `mockall` - Mock object framework
- `pretty_assertions` - Better assertion output formatting

## Development Notes

- The project is designed as a learning exercise in container technology
- Currently focused on documentation improvement, code refactoring, and error handling enhancement
- **Test Framework**: Comprehensive unit test suite with 19 tests covering all core modules
- **Code Quality**: All tests pass and maintain test isolation using temporary directories
- Logging is configured with `env_logger` and `console-subscriber` for debugging

## Recent Improvements

### Test Infrastructure (2025-08-16)
- Added comprehensive unit test suite with 19 tests
- Implemented proper test isolation using `tempfile::TempDir`
- Fixed IP allocation logic in network management
- Resolved message serialization issues in IPC layer
- Added Makefile with testing, building, and development targets

### Key Fixes
- **IP Allocation**: Fixed gateway allocation to use correct bitmap indexing
- **Message Protocol**: Resolved hardcoded length issues in serialization tests
- **Storage Tests**: Implemented proper test isolation to prevent data pollution
- **Container Support**: Verified compilation and testing in Ubuntu container environment
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
- **38 unit tests** covering all core modules (cmd, msg, network/ipam, metas/*)
- **Enhanced metadata system tests**: WAL operations, storage management, integrity verification
- **Concurrency testing**: DashMap deadlock prevention and async operation safety
- Integration tests using `tempfile` for test isolation
- Mock testing with `tokio-test` and `mockall`
- Test coverage includes IP allocation, message serialization, and advanced metadata operations

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
  - `metas/` - **Enhanced metadata and storage management system**
    - `meta.rs` - Container metadata models and management APIs
    - `storage.rs` - Storage operations and transaction management  
    - `wal.rs` - Write-ahead logging with compression and integrity verification
    - `example.rs` - Usage examples and event handler implementations
  - `cmd.rs` - Command definitions and parsing
  - `msg.rs` - Message protocol for client-daemon communication

- **`src/front/`** - Frontend interfaces
  - `cli.rs` - Command-line interface using clap
  - `ops.rs` - Client-side operation implementations

### Key Patterns

1. **Async Architecture**: Uses Tokio runtime for async operations
2. **Message Protocol**: Custom serialization with bincode for client-daemon communication
3. **Enhanced Metadata Management**: WAL+Snapshot architecture for persistent storage
   - **Write-Ahead Logging (WAL)**: All operations logged for durability and recovery
   - **Snapshot System**: Periodic state snapshots with automatic cleanup
   - **Event System**: Real-time metadata change notifications (framework in place)
   - **Advanced Queries**: Multi-dimensional filtering and resource aggregation
   - **Batch Operations**: Atomic multi-operation transactions
4. **Resource Management**: Integration with Linux cgroups and networking primitives

### Important Paths
- Container root: `/tmp/rtain`
- Daemon socket: `/tmp/rtain_daemons.sock`
- Network config: `/tmp/rtain/net/networks`
- **Metadata storage**: `/tmp/rtain/metadata/`
  - WAL files: `current.wal`, `archive/wal-*.log`
  - Snapshots: `snapshots/snapshot-*.json`

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
- `rstest` - Parameterized testing framework
- `flate2` - Compression support for WAL testing

## Development Notes

- The project is designed as a learning exercise in container technology
- Currently focused on documentation improvement, code refactoring, and error handling enhancement
- **Test Framework**: Comprehensive unit test suite with 38 tests covering all core modules
- **Code Quality**: All tests pass and maintain test isolation using temporary directories
- **Enhanced Metadata System**: Advanced container metadata management with WAL+Snapshot architecture
- Logging is configured with `env_logger` and `console-subscriber` for debugging

## Enhanced Metadata System Features

### Container Metadata Management
- **Comprehensive Container Model**: 20+ fields including state, resources, network config, mounts
- **Lifecycle State Tracking**: Created, Running, Stopped, Paused, Restarting states
- **Resource Configuration**: CPU, memory limits, and cgroup settings
- **Network Integration**: Network attachment/detachment with configuration persistence
- **Mount Point Management**: Volume and bind mount tracking with validation

### Persistence Architecture
- **Write-Ahead Logging (WAL)**:
  - Durable operation logging with binary serialization
  - Integrity verification with checksum validation
  - Automatic rotation and archival with configurable retention
  - Compression support for storage efficiency
  - Batch operation support for atomic transactions

- **Snapshot System**:
  - Periodic state snapshots for fast recovery
  - Automatic cleanup with configurable retention policies
  - JSON serialization for human-readable backups
  - Recovery integration with WAL replay

### Advanced Operations
- **Batch Transactions**: Atomic multi-operation updates with rollback
- **Advanced Queries**: Filter by status, labels, creation time, resource usage
- **Resource Aggregation**: CPU, memory, and storage usage summaries
- **Event System Framework**: Infrastructure for real-time change notifications
- **Concurrent Safety**: Thread-safe operations with proper lock management

### Testing Coverage
- **Storage Operations**: All CRUD and advanced operations tested
- **WAL System**: Write, read, integrity, compaction, and validation tests
- **Concurrency**: DashMap deadlock prevention and async safety verification
- **Error Handling**: Edge cases, corruption recovery, and validation
- **Integration**: Cross-component data consistency and recovery scenarios

## Recent Improvements

### Enhanced Metadata System (2025-08-17)
- **Comprehensive metadata management**: Expanded from 5 to 20+ container metadata fields
- **WAL+Snapshot architecture**: Implemented robust persistence with integrity verification
- **Advanced storage operations**: Environment, labels, resources, network, and mount management
- **Event system framework**: Real-time metadata change notification system (foundation)
- **Batch operations**: Atomic multi-operation transactions with rollback support
- **Advanced querying**: Multi-dimensional filtering with resource aggregation
- **Test coverage expansion**: Increased from 19 to 38 tests covering all new functionality
- **Concurrency safety**: Fixed DashMap deadlock issues and improved async operation safety
- **English documentation**: Converted all Chinese content to comprehensive English docs

### Test Infrastructure (2025-08-16)
- Added comprehensive unit test suite with proper test isolation
- Fixed IP allocation logic in network management
- Resolved message serialization issues in IPC layer
- Added Makefile with testing, building, and development targets

### Key Technical Fixes
- **DashMap Deadlock**: Resolved reference lifetime issues in concurrent operations
- **WAL Integrity**: Fixed floating-point precision issues in verification tests
- **Storage Operations**: Implemented proper scoped reference management
- **Test Isolation**: All tests use `tempfile::TempDir` for data isolation
- **Container Support**: Verified compilation and testing in Ubuntu container environment
# Rtain

`Rtain` is a learning project that implements a lightweight container manager in `Rust`. The primary goal of this project is to understand container technology and Linux system programming through a well-structured implementation.

## Features

- Container lifecycle management (create, start, stop, delete)
- Command-line interface for container operations
- Asynchronous runtime powered by Tokio

## Requirements

- Linux operating system
- Rust toolchain (2021 edition or later)
- Root privileges for container operations

## Installation
TODO

## Usage
TODO

## Project Structure

The project is organized with a clear separation of concerns:

```
rtain/
├── src/
│   ├── core/           # Core container management functionality
│   │   ├── container/  # Container implementation
│   │   ├── network/    # Network management
│   │   ├── metas/      # Metadata management
│   │   ├── cmd.rs      # Command processing
│   │   └── msg.rs      # Message handling
│   ├── front/          # Frontend interfaces
│   │   ├── cli.rs      # Command-line interface
│   │   └── ops.rs      # Operations implementation
│   └── bin/            # Binary executables
```

## Development Status

Current focus:
- [ ] Documentation improvement
- [ ] Code refactoring for better maintainability
- [ ] Error handling enhancement
- [ ] Output formatting improvement
- [ ] Test coverage expansion

## Contributing

This is primarily a learning project, but suggestions and improvements are welcome! Please feel free to open issues or submit Pull Requests.
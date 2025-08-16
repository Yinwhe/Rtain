# Rtain Container Manager Makefile

.PHONY: all build test test-unit test-integration test-e2e clean help

# Default target
all: build test

# Build all binaries
build:
	cargo build --release

# Build debug binaries
build-debug:
	cargo build

# Run all tests
test: test-unit test-integration

# Run unit tests only
test-unit:
	cargo test --lib

# Run integration tests
test-integration:
	cargo test --test integration_test

# Run end-to-end tests (requires Linux and potentially root)
test-e2e:
	@echo "Running E2E tests (may require root privileges)"
	cargo test --test e2e_test

# Run tests with verbose output
test-verbose:
	cargo test --lib -- --nocapture
	cargo test --test integration_test -- --nocapture

# Run specific test
test-one TEST:
	cargo test $(TEST) -- --nocapture

# Check code formatting
fmt-check:
	cargo fmt -- --check

# Format code
fmt:
	cargo fmt

# Run clippy lints
lint:
	cargo clippy -- -D warnings

# Run security audit
audit:
	cargo audit

# Clean build artifacts
clean:
	cargo clean

# Run tests in Docker (for consistent Linux environment)
test-docker:
	docker build -t rtain-test -f tests/Dockerfile .
	docker run --rm --privileged rtain-test

# Install development dependencies
install-dev-deps:
	cargo install cargo-audit
	cargo install cargo-watch

# Watch for changes and run tests
watch:
	cargo watch -x "test --lib"

# Generate test coverage report
coverage:
	cargo tarpaulin --out html

# Run benchmarks (if any)
bench:
	cargo bench

help:
	@echo "Available targets:"
	@echo "  build          - Build release binaries"
	@echo "  build-debug    - Build debug binaries"
	@echo "  test           - Run unit and integration tests"
	@echo "  test-unit      - Run unit tests only"
	@echo "  test-integration - Run integration tests"
	@echo "  test-e2e       - Run end-to-end tests (Linux + root required)"
	@echo "  test-verbose   - Run tests with verbose output"
	@echo "  test-one TEST  - Run specific test"
	@echo "  fmt            - Format code"
	@echo "  fmt-check      - Check code formatting"
	@echo "  lint           - Run clippy lints"
	@echo "  audit          - Run security audit"
	@echo "  clean          - Clean build artifacts"
	@echo "  test-docker    - Run tests in Docker"
	@echo "  watch          - Watch for changes and run tests"
	@echo "  coverage       - Generate test coverage report"
	@echo "  help           - Show this help message"
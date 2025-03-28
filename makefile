# Makefile

# Load environment variables from .env
include .env
export $(shell sed 's/=.*//' .env) # Export all variables from .env


# Build target
build:
	@echo "Building Rust project..."
	@echo "Ankr_Api_Key = $(Ankr_Api_Key)"
	@echo "Llama_Api_Key = $(Llama_Api_Key)"
	@echo "DRPC_Api_Key = $(DRPC_Api_Key)"
	@echo "Alchemy_Api_Key = $(Alchemy_Api_Key)"

	cargo build --release --target wasm32-unknown-unknown --package evm_minter
	candid-extractor target/wasm32-unknown-unknown/release/evm_minter.wasm > evm_minter.did


test:
	@echo "Starting the test..."
	@echo "Ankr_Api_Key = TEST_ANKR_API_4r0uqefpoanfijsbflkfslfjsnlfks"
	@echo "Llama_Api_Key = TEST_Llama_API_4r0uqefpoanfijsbflkfs"
	@echo "DRPC_Api_Key = TEST_DRPC_API_4r0uqefpoanfijsbflkfslfjsnlfks"
	@echo "Alchemy_Api_Key = TEST_Alchemy_API_4r0uqefpoanfijsbflkfslfjsnlfks"
	@echo "Building Rust project..."
	cargo build --release --target wasm32-unknown-unknown --package evm_minter
	candid-extractor target/wasm32-unknown-unknown/release/evm_minter.wasm > evm_minter.did
	cargo test

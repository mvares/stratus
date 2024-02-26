import '.justfile_helpers' # _lint, _outdated

postgres_url := env("POSTGRES_URL", "postgres://postgres:123@0.0.0.0:5432/stratus")
testnet_url  := "https://rpc.testnet.cloudwalk.io/"

# Project: Show available tasks
default:
    just --list --unsorted

# Project: Run project setup
setup:
    @echo "* Installing Cargo killport"
    cargo install killport

    @echo "* Installing Cargo wait-service"
    cargo install wait-service

    @echo "* Installing Cargo flamegraph"
    cargo install flamegraph

    @echo "* Cloning Solidity repositories"
    just contracts-clone

# ------------------------------------------------------------------------------
# Stratus tasks
# ------------------------------------------------------------------------------

# Stratus: Run main service with debug options
run *args="":
    #!/bin/bash
    RUST_LOG={{env("RUST_LOG", "stratus=info")}} cargo run --bin stratus -- --enable-genesis --enable-test-accounts {{args}}
    exit 0

# Stratus: Run main service with release options
run-release *args="":
    RUST_LOG={{env("RUST_LOG", "stratus=info")}} cargo run --release --bin stratus -- --enable-genesis --enable-test-accounts {{args}}

run-substrate-mock:
    npm init -y
    npm install csv-parse pg express
    node ./e2e/substrate-sync-mock-server/index.js

# Stratus: Compile with debug options
build:
    cargo build

# Stratus: Compile with release options
build-release:
    cargo build --release

# Stratus: Check, or compile without generating code
check:
    cargo check

# Stratus: Clean build artifacts
clean:
    cargo clean

# Stratus: Build documentation
doc:
    cargo +nightly doc --no-deps

# Stratus: Lint and format code
lint:
    @just _lint

# Stratus: Lint and check code formatting
lint-check nightly-version="":
    @just _lint "{{nightly-version}}" --check "-D warnings"

# Stratus: Compile SQLx queries
sqlx:
    SQLX_OFFLINE=true cargo sqlx prepare --database-url {{postgres_url}} -- --all-targets

# Stratus: Check for outdated crates
outdated:
    @just _outdated

# Stratus: Update only the project dependencies
update:
    cargo update stratus

# ------------------------------------------------------------------------------
# Importer tasks
# ------------------------------------------------------------------------------
# Importer: Download external RPC blocks to temporary storage
importer-download *args="":
    cargo run --bin importer-download --release -- --postgres {{postgres_url}} --external-rpc {{testnet_url}} {{args}}

# Importer: Import downloaded external RPC blocks to Stratus storage
importer-import *args="":
    cargo run --bin importer-import --release -- --postgres {{postgres_url}} {{args}}

# ------------------------------------------------------------------------------
# Test tasks
# ------------------------------------------------------------------------------

# Test: Execute all Rust tests
test name="":
    @just test-doc {{name}}
    @just test-unit {{name}}
    @just test-int {{name}}

# Test: Execute Rust doc tests
test-doc name="":
    cargo test {{name}} --doc

# Test: Execute Rust unit tests
test-unit name="":
    cargo test --lib {{name}} -- --nocapture

# Test: Execute Rust integration tests
test-int name="":
    cargo test --test '*' {{name}} -- --nocapture

# ------------------------------------------------------------------------------
# E2E tasks
# ------------------------------------------------------------------------------

# E2E: Execute Hardhat tests in the specified network
e2e network="stratus" test="":
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi
    if [ ! -d node_modules ]; then
        npm install
    fi

    if [ -z "{{test}}" ]; then
        npx hardhat test test/*.test.ts --network {{network}}
    else
        npx hardhat test test/*.test.ts --network {{network}} --grep "{{test}}"
    fi

# E2E: Starts and execute Hardhat tests in Anvil
e2e-anvil test="":
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi

    echo "-> Starting Anvil"
    anvil --chain-id 2008 --gas-price 0 --block-base-fee-per-gas 0 --port 8546 &

    echo "-> Waiting Anvil to start"
    wait-service --tcp localhost:8546 -- echo

    echo "-> Running E2E tests"
    just e2e anvil {{test}}

    echo "-> Killing Anvil"
    killport 8546

# E2E: Starts and execute Hardhat tests in Hardhat
e2e-hardhat test="":
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi

    echo "-> Starting Hardhat"
    npx hardhat node &

    echo "-> Waiting Hardhat to start"
    wait-service --tcp localhost:8545 -- echo

    echo "-> Running E2E tests"
    just e2e hardhat {{test}}

    echo "-> Killing Hardhat"
    killport 8545

# E2E: Starts and execute Hardhat tests in Stratus
e2e-stratus test="":
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi

    echo "-> Starting Stratus"
    RUST_LOG=info just run -a 0.0.0.0:3000 &

    echo "-> Waiting Stratus to start"
    wait-service --tcp 0.0.0.0:3000 -t 300 -- echo

    echo "-> Running E2E tests"
    just e2e stratus {{test}}
    result_code=$?

    echo "-> Killing Stratus"
    killport 3000
    exit $result_code

# E2E: Starts and execute Hardhat tests in Stratus
e2e-stratus-postgres test="":
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi

    echo "-> Starting Postgres"
    docker-compose down
    docker-compose up -d

    echo "-> Waiting Postgres to start"
    wait-service --tcp 0.0.0.0:5432 -t 300 -- echo

    echo "-> Starting Stratus"
    RUST_LOG=debug just run -a 0.0.0.0:3000 -s {{postgres_url}} > stratus.log &

    echo "-> Waiting Stratus to start"
    wait-service --tcp 0.0.0.0:3000 -t 300 -- echo

    echo "-> Running E2E tests"
    just e2e stratus {{test}}
    result_code=$?

    echo "-> Killing Stratus"
    killport 3000

    echo "-> Killing Postgres"
    docker-compose down

    echo "** -> Stratus log accessible in ./stratus.log **"
    exit $result_code

# E2E: Lint and format code
e2e-lint:
    #!/bin/bash
    if [ -d e2e ]; then
        cd e2e
    fi
    node_modules/.bin/prettier . --write

# E2E: profiles rpc sync and generates a flamegraph
e2e-flamegraph:
    # Start PostgreSQL with Docker Compose
    echo "Starting PostgreSQL with Docker Compose..."
    docker-compose down
    docker-compose up -d --force-recreate

    # Wait for PostgreSQL to be ready
    echo "Waiting for PostgreSQL to be ready..."
    wait-service --tcp 0.0.0.0:5432 -t 300 -- echo
    echo "PostgreSQL is ready."

    # Start the substrate mock server in the background
    echo "Starting substrate mock server..."
    killport 3003
    (just run-substrate-mock &) && \
    echo "Waiting for the substrate mock server to be ready..."
    wait-service --tcp 0.0.0.0:3003 -t 300 -- echo
    echo "Substrate mock server is ready."

    # Run cargo flamegraph with necessary environment variables
    echo "Running cargo flamegraph..."
    sudo CARGO_PROFILE_RELEASE_DEBUG=true cargo flamegraph --bin rpc-server-poller -- --external-rpc=http://localhost:3003/rpc


# ------------------------------------------------------------------------------
# Contracts tasks
# ------------------------------------------------------------------------------

# Contracts: Clone Solidity repositories
contracts-clone:
    cd e2e-contracts && ./clone-contracts.sh

# Contracts: Compile selected Solidity contracts
contracts-compile:
    cd e2e-contracts && ./compile-contracts.sh

# Contracts: Test selected Solidity contracts on Stratus
contracts-test:
    cd e2e-contracts && ./test-contracts.sh
alias e2e-contracts := contracts-test

# Contracts: Run BRLCToken contract tests
contracts-test-brlc-token:
    cd e2e-contracts && ./test-contracts.sh -t

# Contracts: Run BRLCPeriphery contract tests
contracts-test-brlc-periphery:
    cd e2e-contracts && ./test-contracts.sh -p

# Contracts: Run BRLCMultisig contract tests
contracts-test-brlc-multisig:
    cd e2e-contracts && ./test-contracts.sh -m

# Contracts: Run CompoundPeriphery contract tests
contracts-test-brlc-compound:
    cd e2e-contracts && ./test-contracts.sh -c

# Contracts: Remove all the cloned repositories
contracts-remove:
    cd e2e-contracts && ./remove-contracts.sh

# Contracts: Start Stratus and run contracts test
contracts-test-stratus:
    #!/bin/bash
    echo "-> Starting Stratus"
    RUST_LOG=info just run -a 0.0.0.0:3000 > /dev/null &

    echo "-> Waiting Stratus to start"
    wait-service --tcp 0.0.0.0:3000 -t 300 -- echo

    echo "-> Running E2E Contracts tests"
    just e2e-contracts
    result_code=$?

    echo "-> Killing Stratus"
    killport 3000
    exit $result_code

# Contracts: Start Stratus with Postgres and run contracts test
contracts-test-stratus-postgres:
    #!/bin/bash
    echo "-> Starting Postgres"
    docker-compose down
    docker-compose up -d

    echo "-> Waiting Postgres to start"
    wait-service --tcp 0.0.0.0:5432 -t 300 -- echo

    echo "-> Starting Stratus"
    RUST_LOG=debug just run-release -a 0.0.0.0:3000 -s {{postgres_url}} > stratus.log &

    echo "-> Waiting Stratus to start"
    wait-service --tcp 0.0.0.0:3000 -t 300 -- echo

    echo "-> Running E2E tests"
    just e2e-contracts
    result_code=$?

    echo "-> Killing Stratus"
    killport 3000

    echo "-> Killing Postgres"
    docker-compose down

    echo "** -> Stratus log accessible in ./stratus.log **"
    exit $result_code

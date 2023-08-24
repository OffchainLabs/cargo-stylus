# Cargo Stylus 

[![linux](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/linux.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/linux.yml) [![mac](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/mac.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/mac.yml) [![windows](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/windows.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/windows.yml) [![lint](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/check.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/check.yml)

A cargo subcommand for building, verifying, and deploying Arbitrum Stylus WASM programs.

[toc]

## Quick Start

### Installing With Cargo

cargo install --git https://github.com/OffchainLabs/cargo-stylus

### Overview

The cargo stylus command comes with two useful commands `check` and `deploy` for developing and deploying Stylus programs
to Arbitrum chains. Here's a common workflow: 

TODO:

## Compiling and Checking Stylus Programs

**cargo stylus check**

Usage:

```
Instrument a Rust project using Stylus. This command runs compiled WASM code 
through Stylus instrumentation checks and reports any failures

Usage: cargo stylus check [OPTIONS]

Options:
  -e, --endpoint <ENDPOINT>
          The endpoint of the L2 node to connect to [default: http://localhost:8545]
      --wasm-file-path <WASM_FILE_PATH>
          If desired, it loads a WASM file from a specified path. If not provided, it will try to find a WASM file under the current working directory's Rust target release directory and use its contents for the deploy command
      --activate-program-address <ACTIVATE_PROGRAM_ADDRESS>
          Specify the program address we want to check activation for. If unspecified, it will compute the next program address from the user's wallet address and nonce. To avoid needing a wallet to run this command, pass in 0x0000000000000000000000000000000000000000 or any other desired program address to check against
      --private-key-path <PRIVATE_KEY_PATH>
          Privkey source to use with the cargo stylus plugin
      --keystore-path <KEYSTORE_PATH>

      --keystore-password-path <KEYSTORE_PASSWORD_PATH>
```

## Deploying Stylus Programs

**cargo stylus deploy**

Usage:

```
Instruments a Rust project using Stylus and by outputting its brotli-compressed WASM code. Then, it 
submits two transactions: the first deploys the WASM program to an address and the second triggers 
an activation onchain Developers can choose to split up the deploy and activate steps via this command as desired

Usage: cargo stylus deploy [OPTIONS]

Options:
      --estimate-gas-only
          Does not submit a transaction, but instead estimates the gas required to complete the operation
      --mode <MODE>
          By default, submits two transactions to deploy and activate the program to Arbitrum. Otherwise, a user could choose to split up the deploy and activate steps into individual transactions [possible values: deploy-only, activate-only]
  -e, --endpoint <ENDPOINT>
          The endpoint of the L2 node to connect to [default: http://localhost:8545]
      --keystore-path <KEYSTORE_PATH>

      --keystore-password-path <KEYSTORE_PASSWORD_PATH>

      --private-key-path <PRIVATE_KEY_PATH>
          Privkey source to use with the cargo stylus plugin
      --activate-program-address <ACTIVATE_PROGRAM_ADDRESS>
          If only activating an already-deployed, onchain program, the address of the program to send an activation tx for
      --wasm-file-path <WASM_FILE_PATH>
          If desired, it loads a WASM file from a specified path. If not provided, it will try to find a WASM file under the current working directory's Rust target release directory and use its contents for the deploy command
```

### Optimizing Binary Sizes

Stylus programs must fit within the 24Kb code-size limit of Ethereum smart contracts. By default, the cargo stylus tool will attempt to compile a Rust program into WASM with reasonable optimizations. However, there are additional options available in case a program exceeds the 24Kb limit from using default settings.

## Alternative Installations

### Docker Images

TODO:

### Precompiled Binaries

TODO:

## License

Cargo Stylus is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

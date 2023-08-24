# Cargo Stylus 

[![linux](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/linux.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/linux.yml) [![mac](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/mac.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/mac.yml) [![windows](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/windows.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/windows.yml) [![lint](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/check.yml/badge.svg)](https://github.com/OffchainLabs/cargo-stylus/actions/workflows/check.yml)

A cargo subcommand for building, verifying, and deploying Arbitrum Stylus WASM programs in Rust.

## Quick Start

### Installing With Cargo

Install [Rust](https://www.rust-lang.org/tools/install), and then install the plugin using the Cargo tool:

```
cargo install --git https://github.com/OffchainLabs/cargo-stylus
```

You should now have it available as a Cargo subcommand:

```
cargo stylus --help

Cargo command for developing Arbitrum Stylus projects

Usage: cargo stylus check | cargo stylus deploy
```

### Overview

The cargo stylus command comes with two useful commands `check` and `deploy` for developing and deploying Stylus programs
to Arbitrum chains. Here's a common workflow: 

Clone the [hello-stylus]() repository locally and change directory into it:

TODO:

Then, develop your Rust program normally and take advantage of all the features the [stylus-sdk](https://github.com/OffchainLabs/stylus-sdk-rs) has to offer. To check whether or not your program will successfully deploy and activate onchain, use the `cargo stylus check` subcommand:

```
cargo stylus check --endpoint=<JSON_RPC_ENDPOINT> --activate-program-address=<DESIRED_PROGRAM_ADDRESS>
```

This command will attempt to verify that your program can be deployed and activated onchain without requiring a transaction by specifying a JSON-RPC endpoint and an expected program address for your deployment. You can set this address to `0x0000000000000000000000000000000000000000` for local testing, or connect your wallet / private key to use your next, expected contract address for this check. See `cargo stylus check --help` for more options.

If the command above fails, you'll see detailed information about why your WASM will be rejected:

```
```

If your program succeeds, you'll see the following message:

```
```

Once you're ready to deploy your program onchain, you can use the `cargo stylus deploy` subcommand as follows:

First, we can estimate the gas required to perform our deployment and activation with:

```
cargo stylus deploy --private-key-path=<PRIVKEY_FILE_PATH> --endpoint=<JSON_RPC_ENDPOINT> --estimate-gas
```

Next, attempt an actual deployment. Two transactions will be sent onchain.

```
cargo stylus deploy --private-key-path=<PRIVKEY_FILE_PATH> --endpoint=<JSON_RPC_ENDPOINT> --estimate-gas
```

## Compiling and Checking Stylus Programs

**cargo stylus check**

Instruments a Rust project using Stylus. This command runs compiled WASM code through Stylus instrumentation checks and reports any failures. It verifies the program can compile onchain by making an call to a Arbitrum chain RPC endpoint.

```
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

Instruments a Rust project using Stylus and by outputting its brotli-compressed WASM code. Then, it submits two transactions: the first deploys the WASM program to an address and the second triggers an activation onchain Developers can choose to split up the deploy and activate steps via this command as desired.


```
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

Brotli-compressed, Stylus program WASM binaries must fit within the **24Kb** [code-size limit](https://ethereum.org/en/developers/tutorials/downsizing-contracts-to-fight-the-contract-size-limit/) of Ethereum smart contracts. By default, the cargo stylus tool will attempt to compile a Rust program into WASM with reasonable optimizations. However, there are additional options available in case a program exceeds the 24Kb limit from using default settings. Deploying smaller binaries onchain is cheaper and better for the overall network, as deployed WASM programs will exist on the Arbitrum chain's storage forever. 

We recommend optimizing your Stylus program's sizes as much as is reasonable, but keep in mind the safety tradeoffs of using some of the more advanced optimizations.

For a deep-dive into the different options for optimizing binary sizes using cargo stylus, see [OPTIMIZING_BINARIES.md](./OPTIMIZING_BINARIES.md).

## Alternative Installations

### Docker Images

TODO:

### Precompiled Binaries

TODO:

## License

Cargo Stylus is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

# Cargo Stylus

A cargo subcommand for building, verifying, and deploying Arbitrum Stylus WASM programs.

[toc]

## Quick Start

### Installing With Cargo

cargo install --git https://github.com/OffchainLabs/cargo-stylus

### Overview

The cargo stylus command comes with two useful commands for developing and deploying Stylus programs
to Arbitrum chains.

`cargo stylus check`

Usage:

```
```

`cargo stylus deploy`

Usage:

```
```

## Compiling and Checking Stylus Programs

## Deploying Stylus Programs

### Optimizing Binary Sizes

Stylus programs must fit within the 24Kb code-size limit of Ethereum smart contracts. By default, the cargo stylus tool will attempt to compile a Rust program into WASM with reasonable optimizations. However, there are additional options available in case a program exceeds the 24Kb limit from using default settings.

## Alternative Installations

### Docker Images

TODO:

### Precompiled Binaries

TODO:

## License

Cargo Stylus is distributed under the terms of both the MIT license and the Apache License (Version 2.0).

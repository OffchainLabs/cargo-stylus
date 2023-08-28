# Invalid Stylus WASM Programs

This document explains the limitations of Stylus WASM programs and why certain programs might fail `cargo stylus check`. Stylus programs are bound by similar rules to Ethereum smart contracts when it comes to bounded execution, bounded memory use, and determinism. 

WASM programs must fit within the **24Kb** code size limit of Arbitrum chains _after_ compression. Uncompressed WASMs must have a size less than **128Kb**.

While Stylus includes a large portion of available WASM opcodes, not all of them are supported. To see the set of allowed / disallowed opcodes, see [here](https://github.com/OffchainLabs/stylus/blob/stylus/arbitrator/prover/src/wavm.rs#L731).

When a user WASM gets "activated" on chain, it goes through a series of checks through the Stylus codebase including, but not limited to the following:

1. **Depth checking** the WASM code to ensure stack overflows are deterministic across different compilers and targets
2. Meter the WASM for bounded execution through **ink** - the Stylus analogue of gas units
3. Meter the WASM for **memory use** to ensure it is within maximum bounds
4. Check for any **reserved symbols** being used
5. Check for **disallowed opcodes**, such as SIMD or other features
6. Disallow WASMs with an enormous amount of **functions and exports**

Stylus programs should use `#[no_std]` to avoid including the Rust standard library and keep code small. Many crates that build without the standard library make for great dependencies to use in Stylus programs, as long as the total, compressed WASM size is within the 24Kb code size limit.

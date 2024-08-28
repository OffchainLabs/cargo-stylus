// Copyright 2023-2024, Offchain Labs, Inc.
// For licensing, see https://github.com/OffchainLabs/cargo-stylus/blob/main/licenses/COPYRIGHT.md

macro_rules! greyln {
    ($($msg:expr),*) => {{
        let msg = format!($($msg),*);
        println!("{}", msg.grey())
    }};
}

macro_rules! mintln {
    ($($msg:expr),*) => {{
        let msg = format!($($msg),*);
        println!("{}", msg.mint())
    }};
}

macro_rules! egreyln {
    ($($msg:expr),*) => {{
        let msg = format!($($msg),*);
        eprintln!("{}", msg.grey())
    }};
}

pub(crate) use {egreyln, greyln, mintln};

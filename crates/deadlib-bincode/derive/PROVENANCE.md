# Source provenance

This crate starts from the exact `bincode_derive 2.0.1` crates.io archive:

- SHA-256: `bf95709a440f45e986983918d0e8a1f30a9b1df04918fc828670606804ac3c09`
- Published VCS commit: `4673360aa638b2b907dff24538de54f258d157da`

The package was renamed to `deadlib-bincode-derive`, while the Rust crate name
remains `bincode_derive`. Its abandoned `virtue` dependency was replaced by the
required implementation from the exact `virtue 0.0.18` crates.io archive:

- SHA-256: `051eb1abcf10076295e815102942cc58f9d5e3b4560e46e53c21e8ff6f3af7b1`
- Published VCS commit: `1ecc01325a038c4f28aa8de25caae7c6eb91c66f`

Virtue is now a private implementation module rather than a Cargo package.
Unused portions of its former public helper API were removed. Explicit output
lifetimes were added where required by current Rust diagnostics; macro output is
unchanged.

The original bincode-derive and Virtue READMEs and MIT licenses are retained
beside this file.

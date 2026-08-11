# deadlib-bincode-derive

Derive macros for `deadlib-bincode`, based exactly on `bincode_derive 2.0.1`.
The required portions of Virtue are embedded privately in this package, so it
does not add another maintenance crate or depend on abandoned
bincode-organization packages.

Use this through `deadlib-bincode`; consumers normally should not depend on this
package directly. Source hashes and adaptation notes are recorded in
[`PROVENANCE.md`](PROVENANCE.md).

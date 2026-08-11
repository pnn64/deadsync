# Security policy

`deadlib-bincode` is maintained for DeadSync and compatible consumers. The
supported release line is the latest published `2.0.x` release.

Report suspected vulnerabilities privately through the Deadsync repository's
GitHub security-advisory interface. Use a public issue only when the report does
not expose a vulnerability before a fix is available.

Security fixes must preserve the bincode 2 wire format whenever possible. If a
safe fix requires a format or API break, it will ship as a major release with a
documented migration path.

Decoding hostile data without a configured byte limit is outside the supported
threat model. Consumers must use `Configuration::with_limit` with a limit
appropriate to their schema and transport.

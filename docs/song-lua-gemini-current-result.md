# Gemini in Clockland current result

The supplied `Gemini in Clockland/Gemini in Clockland.ssc` passes 7,117/7,117 existing
comparisons in MAIN's isolated verification checkout.

The geometry audit now uses unit dimensions for a Quad, matching production
rendering. This older reference stores already zoomed Quad dimensions in
its texture size field. Multiplying those dimensions by zoom again caused
the false bounds mismatch. All five geometry checks remain active, and a
negative regression confirms that doubling the actual width still fails.

The existing reference remains unchanged. No resources were downloaded,
no ITGmania window was launched, and frozen project identities are retained.
This headless comparison does not establish full gameplay pixel parity.

Promoted MAIN version: `0.5.1784`.

# RGD-U9 Historical Windows Capture

This directory preserves the surviving local inputs and outputs behind the `b2ddb5b` Windows
automatic measurement slice. It does not close RGD-U9.

The JSON/JSONL files are LF-normalized semantic copies for review. Each matching
`*.original.base64` file reconstructs the exact original Windows bytes whose SHA-256 is recorded in
`import-receipt.json`. Verification also requires each semantic copy to equal the UTF-8 transport
after newline normalization. The original transport bytes remain available without weakening the
repository-wide LF policy for canonical JSON.

The executed one-run collector was not committed and contained host-specific absolute paths. Its
digest is retained, but its source is intentionally not presented as a reusable tool. A new
parameterized collector must be committed, tested, and used at the integrated source revision
before RGD-U9 can again be called reproducible or consumed by RGD-U11.

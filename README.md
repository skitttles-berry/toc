<div align="center">
  <h1>toc</h1>
  <p><strong>TUI Object Converter</strong></p>
  <p>A TUI and CLI tool for converting text and bytes between formats.</p>
  <p><code>TUI</code> · <code>CLI</code> · <code>Local-only</code> · <code>36 transforms</code></p>
  <p>English · <a href="README.ko.md">한국어</a></p>
</div>

## Get started in 30 seconds

### Install
```bash
git clone https://github.com/skitttles-berry/toc.git
cd toc
cargo install --locked --path .
```

### TUI
```bash
toc tui
```

### CLI
```bash
printf '%s' 'hello' | toc base64-encode
printf '%s' '%7B%22name%22%3A%22toc%22%7D' | toc url-decode --then format-json
```

## TUI usage

The TUI lets you build a transform pipeline and inspect its output without modifying the original input. Enter source text in Input, add transforms to Pipeline, and review the result in Output.

### Preview

![toc TUI demo](docs/asciinema/toc-tui.gif)

### Get started in 4 steps

| Step | Action | How |
|---:|---|---|
| 1 | Enter input | Write the source text in Input |
| 2 | Add a transform | Press <kbd>Ctrl</kbd> + <kbd>p</kbd> to select a transform |
| 3 | Run it | Press <kbd>s</kbd> to run the selected stage |
| 4 | Review | Check the result in Output |

### Output View

| View | Purpose |
|---|---|
| `SMART` | Automatically select a view for the result format |
| `TEXT` | Display UTF-8 text |
| `HEX` | Inspect bytes in Offset, Hex, and ASCII columns |
| `TRACE` | Review each Pipeline stage and a safe failure summary |

### Keyboard shortcuts

| Area | Key | Action |
|---|---|---|
| Global | <kbd>Tab</kbd><br><kbd>Shift</kbd> + <kbd>Tab</kbd> | Move between panels |
|  | <kbd>Ctrl</kbd> + <kbd>p</kbd> | Add a transform |
|  | <kbd>F1</kbd> | Open help |
|  | <kbd>Ctrl</kbd> + <kbd>q</kbd> | Exit cleanly |
|  | <kbd>Esc</kbd> | Close a dialog or zoomed view, or cancel execution |
| Input | <kbd>Cmd</kbd> + <kbd>←</kbd> / <kbd>→</kbd><br><kbd>Home</kbd> / <kbd>End</kbd><br><kbd>Ctrl</kbd> + <kbd>A</kbd> / <kbd>E</kbd> | Move to the start or end of the current logical line |
|  | <kbd>Cmd</kbd> + <kbd>Backspace</kbd> (macOS)<br><kbd>Ctrl</kbd> + <kbd>Backspace</kbd> (Windows and Linux) | Delete from the cursor to the start of the current logical line<br>Join the previous line when already at the start |
|  | <kbd>Option</kbd> + <kbd>←</kbd> / <kbd>→</kbd><br><kbd>Ctrl</kbd> + <kbd>←</kbd> / <kbd>→</kbd> | Move to the previous or next word boundary |
|  | The movement keys above + <kbd>Shift</kbd> | Select the traversed range |
| Pipeline | <kbd>↑</kbd><br><kbd>↓</kbd> | Select a stage |
|  | <kbd>Shift</kbd> + <kbd>↑</kbd><br><kbd>Shift</kbd> + <kbd>↓</kbd> | Move a stage |
|  | <kbd>Space</kbd> | Toggle a stage |
|  | <kbd>Backspace</kbd> | Remove a stage |
|  | <kbd>Enter</kbd> | Inspect a stage |
|  | <kbd>s</kbd> | Run the selected stage |
|  | <kbd>f</kbd> | Restore the final result |
|  | <kbd>z</kbd> | Zoom Pipeline |
| Output | <kbd>Enter</kbd> | Pretty Copy |
|  | <kbd>Shift</kbd> + <kbd>Enter</kbd> | Raw Copy |
|  | <kbd>v</kbd> | Switch View |
|  | <kbd>z</kbd> | Zoom Output |

Some terminals intercept Command or Option combinations as their own shortcuts, or cannot distinguish
<kbd>Ctrl</kbd> + <kbd>Backspace</kbd> from a regular <kbd>Backspace</kbd>.
In those terminals, the aliases for deleting to the start of a line are unavailable. Use <kbd>Home</kbd> or <kbd>End</kbd>,
<kbd>Ctrl</kbd> + <kbd>A</kbd> or <kbd>E</kbd>, and <kbd>Ctrl</kbd> + <kbd>←</kbd> or <kbd>→</kbd>
for cursor movement.

Raw Copy may be limited in terminals that cannot distinguish `Shift+Enter`.

## CLI usage

```console
# Base64-encode a string
$ printf '%s' 'hello' | toc base64-encode
aGVsbG8=

# URL-decode and format JSON
$ printf '%s' '%7B%22name%22%3A%22toc%22%7D' \
  | toc url-decode --then format-json
{
  "name": "toc"
}

# Decode a JSON string, trim it, and convert it to lowercase
$ printf '%s' '"  TOC  "' \
  | toc json-string-decode --then trim --then lowercase
toc

# Format JSON from a file
$ toc format-json --input input.json

# Save binary Gzip output
$ toc gzip-compress --input input.txt > output.gz
```

- The CLI reads from standard input or `--input PATH`.
- It does not append a newline to successful output.
- Redirect binary output to a file instead of writing it directly to the terminal.

## Supported transforms

| Category | Transform ID |
|---|---|
| Encoding | `base64-encode`<br>`base64-decode`<br>`base64url-encode`<br>`base64url-decode`<br>`base32-encode`<br>`base32-decode`<br>`url-encode`<br>`url-decode`<br>`hex-encode`<br>`hex-decode`<br>`html-encode`<br>`html-decode`<br>`json-string-encode`<br>`json-string-decode`<br>`utf16le-encode`<br>`utf16le-decode`<br>`utf16be-encode`<br>`utf16be-decode` |
| Text and data | `trim`<br>`lowercase`<br>`uppercase`<br>`format-json`<br>`minify-json`<br>`rot13`<br>`sort-lines`<br>`remove-duplicate-lines` |
| Security analysis | `url-defang`<br>`url-refang`<br>`jwt-decode`<br>`normalize-ip` |
| Hashing and compression | `sha256`<br>`sha512`<br>`gzip-compress`<br>`gzip-decompress`<br>`zlib-compress`<br>`zlib-decompress` |

- Base64URL encoding omits padding, and `url-decode` leaves `+` unchanged.
- `json-string-decode` accepts exactly one JSON string. The UTF-16 encoders do not add a BOM, and the decoders preserve U+FEFF as a regular character.
- `jwt-decode` does not verify signatures. Gzip and zlib compression produce deterministic output for the same input.
- `zlib-decompress` accepts exactly one dictionary-free RFC 1950 stream and rejects truncated or trailing data.
- `normalize-ip` accepts exactly one address and rejects whitespace, CIDR notation, ports, brackets, and zone identifiers.

## Limits

| Execution path | Input | Output per stage |
|---|---:|---:|
| CLI | 64 MiB | 256 MiB |
| TUI | 1 MiB | 64 MiB |

- A Pipeline can contain up to 32 transforms.

## License

[MIT](LICENSE)

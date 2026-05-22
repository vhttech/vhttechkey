# Verification Checklist

Run these checks before every release.  All items must pass with zero failures,
zero warnings, and zero crashes.

---

## 1. Automated test suite

```bash
cargo test --workspace
```

**Pass criteria**: 0 test failures, 0 ignored tests regressing from baseline.

---

## 2. Clippy — zero warnings

```bash
cargo clippy --workspace -- -D warnings
```

**Pass criteria**: no `warning:` or `error:` lines in output.

---

## 3. Miri — memory safety (nightly)

```bash
cargo +nightly miri test -p vi-core
```

**Pass criteria**: `Miri: test result: ok.  N passed; 0 failed` and no
`error: Undefined Behavior` output.

Note: Miri does not support I/O-heavy crates (vi-daemon, vi-wayland, etc.).
Only vi-core (pure logic) is expected to run cleanly under Miri.

---

## 4. Fuzz — no crashes

```bash
cargo fuzz run fuzz_key_sequence  -- -max_total_time=60
cargo fuzz run fuzz_config        -- -max_total_time=60
cargo fuzz run fuzz_unicode_pipeline -- -max_total_time=60
```

**Pass criteria**: no `CRASH`, `TIMEOUT`, or `OOM` lines; each run ends with
`Done N runs in 60 second(s)`.

---

## 5. Manual end-to-end: Telex typing in four environments

For each app listed below:

1. Ensure vi-daemon is running and the method is set to **Telex**.
2. Open the app and focus a text field.
3. Type `viet nam` (9 keystrokes, no special key).
4. Verify the result reads **`việt nam`** on screen.

| App | Backend | Expected result |
|---|---|---|
| gedit | IBus | `việt nam` |
| Kate | Fcitx5 | `việt nam` |
| foot terminal (`nano /tmp/t.txt`) | Wayland text-input-v3 | `việt nam` |
| xterm | X11 / XIM | `việt nam` |

---

## 6. Python NFC verification

After step 5, check the saved file (use foot/nano output):

```bash
python3 -c "
import unicodedata
text = 'việt nam'
for ch in text:
    cp   = ord(ch)
    name = unicodedata.name(ch, 'UNKNOWN')
    nfc  = unicodedata.normalize('NFC', ch) == ch
    print(f'U+{cp:04X}  {ch!r:3}  {\"NFC\" if nfc else \"NOT NFC\"}  {name}')
"
```

**Pass criteria**: every line reads `NFC`; no `NOT NFC` entries.

Expected output:

```
U+0076  'v'  NFC  LATIN SMALL LETTER V
U+0069  'i'  NFC  LATIN SMALL LETTER I
U+1EC7  'ệ'  NFC  LATIN SMALL LETTER E WITH CIRCUMFLEX AND DOT BELOW
U+0074  't'  NFC  LATIN SMALL LETTER T
U+0020  ' '  NFC  SPACE
U+006E  'n'  NFC  LATIN SMALL LETTER N
U+0061  'a'  NFC  LATIN SMALL LETTER A
U+006D  'm'  NFC  LATIN SMALL LETTER M
```

---

## 7. Valgrind — zero memory leaks

```bash
valgrind \
  --leak-check=full \
  --error-exitcode=1 \
  --suppressions=/usr/share/glib-2.0/valgrind/glib.supp \
  vi-daemon &
DAEMON_PID=$!

# Send 1000 synthetic key events
for i in $(seq 1 1000); do
  echo '{"cmd":"set_method","method":"telex"}' | \
    nc -q1 -U "$XDG_RUNTIME_DIR/vi-daemon.sock" > /dev/null
done

kill $DAEMON_PID
wait $DAEMON_PID
```

**Pass criteria**: valgrind exits with code 0 and reports
`definitely lost: 0 bytes in 0 blocks`.

---

## 8. UI smoke test — VNI mode

1. Launch `vi-ui`.
2. Open the **Input Method** panel.
3. Switch to **VNI** from the dropdown.  Verify the rule summary updates.
4. Open the **Typing Test** panel.
5. Type `81 82 83` (space-separated digit sequences) — with VNI active in the
   system IME, each sequence should produce a Vietnamese character.
6. The typing test text area should contain `ặ ắ ẳ` (U+1EB7 U+0020 U+1EAF
   U+0020 U+1EB3).

**Pass criteria**: all three characters display correctly; the NFC analysis
table shows `✓` for each character and no `U+03xx` combining marks.

---

## Sign-off

| Item | Status | Tester | Date |
|---|---|---|---|
| 1. `cargo test` | | | |
| 2. `cargo clippy` | | | |
| 3. Miri | | | |
| 4. Fuzz | | | |
| 5. Manual E2E | | | |
| 6. Python NFC | | | |
| 7. Valgrind | | | |
| 8. UI smoke | | | |

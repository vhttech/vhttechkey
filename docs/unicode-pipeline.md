# Unicode Pipeline

## Why NFC?

Vietnamese text uses precomposed characters where a single codepoint carries
both the base vowel and any diacritical marks (e.g. `ệ` = U+1EB9, LATIN SMALL
LETTER E WITH CIRCUMFLEX AND DOT BELOW).  Two normalization forms are in common
use:

| Form | Description | Example for "ệ" |
|---|---|---|
| **NFC** | Precomposed — one codepoint per glyph | U+1EB9 |
| **NFD** | Decomposed — base + combining marks | U+0065 U+0323 U+0302 |

vime always outputs **NFC** because:

1. GTK / Qt text widgets store and render NFC internally; inserting NFD causes
   the combining marks to appear as separate characters (double diacritics).
2. The Linux clipboard and most app paste handlers expect NFC.
3. `String::len()` / byte offsets are predictable when each grapheme is one
   codepoint.
4. Unicode collation and string comparison is simpler with NFC.

## Sequence of operations

```
Raw preedit string  (may be ASCII partial sequence, e.g. "vie65t")
        │
        ▼  Step 1 – rule application (vi-core::methods)
        │   The active input method (Telex/VNI/VIQR) replaces trigger sequences
        │   with their target Unicode scalar values.
        │   "vie65t"  →  "viết"  (intermediate, may still be NFD-like)
        │
        ▼  Step 2 – canonical decomposition  (unicode_normalization::nfd())
        │   Every precomposed character is split into base + combining marks.
        │   "viết" → "vie\u{0301}\u{0323}t"  (simplified; actual differs)
        │
        ▼  Step 3 – canonical combining class reorder
        │   Combining marks are sorted by their Canonical Combining Class (CCC).
        │   Marks with lower CCC sort first.  This ensures a unique byte sequence
        │   for the same logical character regardless of input order.
        │
        ▼  Step 4 – NFC composition  (unicode_normalization::nfc())
        │   Adjacent pairs (base, combining) are replaced with their precomposed
        │   codepoint where one exists in the Unicode composition table.
        │   "…\u{0301}\u{0323}…" → "ệ" (U+1EB9)
        │
        ▼
NfcString  (newtype guaranteeing NFC; only constructable inside UnicodePipeline)
```

## Vietnamese codepoints affected

The 134 precomposed Vietnamese codepoints live in two Unicode blocks:

| Block | Range | Codepoints | Examples |
|---|---|---|---|
| Latin Extended Additional | U+1E00–U+1EFF | 128 | ề ế ệ ổ ộ ợ ự ặ ắ ằ ẳ ẫ |
| Latin-1 Supplement | U+00C0–U+00FF | 6 | à á â ã è é |

All of these are **NFC-stable**: their precomposed form is the canonical NFC
representation and will survive a round-trip through the pipeline unchanged.

## Common wrong encodings and their NFC forms

These encodings appear in legacy documents and cause rendering problems in
modern Linux apps.  vime's pipeline corrects them on output.

| Wrong encoding | Codepoints (hex) | Description | Correct NFC | Codepoint |
|---|---|---|---|---|
| a + ◌̣ + ◌̂ | 0061 0323 0302 | NFD, wrong CCC order | ậ | U+1EAD |
| a + ◌̂ + ◌̣ | 0061 0302 0323 | NFD, correct CCC order | ậ | U+1EAD |
| ă + ◌́ | 0103 0301 | NFD partial | ắ | U+1EAF |
| ă + ◌̀ | 0103 0300 | NFD partial | ằ | U+1EB1 |
| ă + ◌̣ | 0103 0323 | NFD partial | ặ | U+1EB7 |
| o + ◌̛ + ◌̣ | 006F 031B 0323 | NFD, COMBINING HORN | ợ | U+1EE3 |
| u + ◌̛ + ◌̣ | 0075 031B 0323 | NFD, COMBINING HORN | ự | U+1EF1 |
| VISCII cp 0xF5 | — | Legacy 8-bit encoding | ợ | U+1EE3 |
| VPS cp 0xD5 | — | Legacy 8-bit encoding | ợ | U+1EE3 |

## Test coverage matrix

| Test suite | Count | Location | What is verified |
|---|---|---|---|
| Golden 216 | 216 | `vi-testing/tests/golden_216.rs` | Every Vietnamese vowel × 6 tones × 3 input methods produces the expected character AND is valid NFC |
| NFD round-trip | 10+ | `vi-testing/tests/unicode_torture.rs` | NFD input (base + combining marks, including reversed CCC order) normalises to the correct NFC codepoint |
| Wrong-order same-CCC marks | 1 | `vi-testing/tests/unicode_torture.rs` | `a+U+0301+U+0302` (acute before circumflex, both CCC=230) produces stable NFC output distinct from the intended `ấ` — documents that input order matters for same-CCC mark pairs |
| Legacy encoding detection | 4+ | `vi-testing/tests/unicode_torture.rs` | C1 control codepoints (U+0080–U+009F) from TCVN3/VPS/VISCII files decoded as Latin-1 are rejected with `CompositionError::LegacyEncoding` |
| Surrogate rejection | 1 | `vi-testing/tests/unicode_torture.rs` | `CompositionError::SurrogateCodepoint` variant exists for FFI/CESU-8 paths; engine output never contains surrogates |
| Unicode torture (pipeline) | 40+ | `vi-testing/src/unicode_torture.rs` + `tests/unicode_torture_test.rs` | Full battery of NFD→NFC, emoji ZWJ, C1 detection, orphaned combining marks, non-character codepoints |
| Property tests | ∞ | `vi-testing/tests/golden_exhaustive_test.rs` | Arbitrary lowercase prefix + golden 216 cases via proptest |

## Legacy encoding detection

The pipeline rejects strings that contain **C1 control characters** (U+0080–U+009F).
These codepoints appear when TCVN3, VPS, or VISCII documents (8-bit encodings)
are decoded byte-by-byte as Latin-1 and re-encoded as UTF-8.  Byte values
0x80–0x9F in those encodings map to Vietnamese characters; when misread as
Latin-1 they become U+0080–U+009F, which have no useful meaning in Unicode text.

| Error | Trigger | Meaning |
|---|---|---|
| `CompositionError::LegacyEncoding(cp)` | Any codepoint in U+0080–U+009F | Caller must re-encode the source document as UTF-8 using the correct code-page table |

Characters in the Latin-1 Supplement block **above** U+00A0 (e.g., `à` = U+00E0,
`ô` = U+00F4) are valid Unicode and pass through the pipeline unchanged.

## Verifying NFC output

```python
import unicodedata

text = "việt nam"
for ch in text:
    cp   = ord(ch)
    name = unicodedata.name(ch, "UNKNOWN")
    form = unicodedata.normalize("NFC", ch) == ch
    print(f"U+{cp:04X}  {ch!r:4}  {'NFC' if form else 'NOT NFC':7}  {name}")
```

Expected output for `"việt nam"` (all NFC):

```
U+0076  'v'  NFC     LATIN SMALL LETTER V
U+0069  'i'  NFC     LATIN SMALL LETTER I
U+1EC7  'ệ'  NFC     LATIN SMALL LETTER E WITH CIRCUMFLEX AND DOT BELOW
U+0074  't'  NFC     LATIN SMALL LETTER T
U+0020  ' '  NFC     SPACE
U+006E  'n'  NFC     LATIN SMALL LETTER N
U+0061  'a'  NFC     LATIN SMALL LETTER A
U+006D  'm'  NFC     LATIN SMALL LETTER M
```

# Table Laboratory — wrapping, formatting, geometry

## Table A: common formatting

| Left | Center | Right | Mixed |
|:-----|:------:|------:|:------|
| **Bold** | *Italic* | 4.5 | [Example](https://example.com) |
| `code` | ***both*** | -800 | ~~removed~~ and normal |
| A \| B | Русский язык | 123456789 | `x \| y` is inline code |
| Small | 🐉 | 0 | A sentence long enough to wrap across at least two lines in a narrow editor viewport. |

## Table B: narrow headings and realistic content

| Exercise | Sets | Reps | Rest | Notes |
|:--|--:|--:|--:|:--|
| Goblet Squat | 3 | 10–12 | 90s | Hold dumbbell at chest; great for learning the movement |
| Dumbbell Bench Press | 3 | 10–12 | 90s | Flat bench, controlled tempo |
| Seated Cable Row | 3 | 10–12 | 90s | Squeeze shoulder blades together |
| Dumbbell Shoulder Press | 3 | 10–12 | 90s | Seated or standing |
| Leg Curl | 2 | 12–15 | 60s | Keep your knee aligned |
| Plank | 3 | 30–45s | 45s | Build up duration over time |

**Known old bug:** `Rest` wrapped into `Res` and `t`, despite lots of room elsewhere, and cell borders did not align. Check at both narrow and wide window widths.

## Table C: lots of columns

| ID | A | B | C | D | E | F | G | H | I | J | K |
|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|:-:|
| 01 | alpha | bravo | charlie | delta | echo | foxtrot | golf | hotel | india | juliet | kilo |
| 02 | 111 | 222 | 333 | 444 | 555 | 666 | 777 | 888 | 999 | 000 | 123 |

Expect either readable horizontal overflow or correct wrapping. The rightmost cells must not disappear or paint over other UI.

## Table D: escaped pipe and code

| Literal source | Expected cell content |
|:--|:--|
| `a \| b` | Inline code should preserve the pipe |
| Left \| Right | A single cell with an escaped pipe |
| **Strong** and *emphasis* | Both styles must project inside a cell |

## Table E: empty cells and unicode

| Name | Description | Value |
|---|---|---:|
| Empty | | 0 |
| | Empty first cell | 1 |
| Уникод | 中日韩 — عربي — é | 2 |
| Last | **end** | 3 |

### Manual loop

1. Scroll through each table, widen and narrow the editor.
2. Click inside the longest cell; type at end, at start, and on wrapped second line.
3. `Ctrl+Z`, `Ctrl+Shift+Z`, save, switch tabs, reopen.
4. Verify Markdown syntax still exists correctly in the source.
5. Screenshot any clipped right edge, misaligned divider, or caret jump.

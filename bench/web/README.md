# Web automation benchmark

Orion against Selenium and Playwright on the task that defines a scraper:
**load a listing and extract several fields from each row**.

```powershell
powershell -ExecutionPolicy Bypass -File bench\web\run_web.ps1
```

Requirements: `python` with `selenium` and `playwright` (`pip install selenium
playwright` — you do **not** need `playwright install`, the Chrome you already
have is used), Chrome installed, and the release binary of Orion.

## The task

500 cards × 4 fields = **2,000 reads**: two texts, an attribute of the row
itself and an attribute of a descendant. One of the texts is a price with
thousands separators, so it has to be converted to a number — in the Python
variants that conversion is written by hand, because it is part of the work
being compared.

The page is loaded as a local file (`file://`). There is no network and no
server: what is measured is the cost of talking to the browser.

## The rules

A benchmark like this is easy to rig without meaning to, so:

1. **The same Chrome binary** for all three. Playwright would download its own
   Chromium; here it is given `executable_path` pointing at the installed one.
   If each one drove a different browser, the number would be measuring that.
2. **All three headless and with no images**, with the same flags.
3. **Two forms per tool.** `idiomatic` is how it appears in each one's
   documentation: locate the elements and ask for their text one by one. `js` is
   the way out known to anyone who has already fought this problem: send an
   evaluation that solves it inside the page.

   Comparing only against the slow form would be a strawman. What is measured is
   **what doing it well costs in each tool**, and which one gives you the good
   path without being asked.
4. **They all print a fingerprint** (SHA-256 of what was extracted, in whole
   cents so it does not depend on how each language formats numbers). If they do
   not match, the script stops: there is no point publishing timings for
   different tasks.
5. Two passes; the **second** is reported, with everything warm.

## What each column measures

- **extraction**: only the reading of data, with the page already loaded. It is
  the clean comparison between ways of talking to the browser.
- **full process**: launch the browser, extract once and close. This is where you
  see what it costs to raise and tear down each stack.
- **process RAM**: peak of the process you write yourself (`python` or
  `orion.exe`).
- **stack RAM**: that process **plus the helpers it launches**.

That last column exists because the previous one, on its own, counts wrong. Each
tool needs a different companion, and it is not the browser:

| | its own helper process |
|---|---|
| Orion | **none** — it speaks CDP from its own process |
| Selenium | `chromedriver.exe`, a second binary whose version has to match Chrome's |
| Playwright | a `node.exe`, because its driver is written in JavaScript |

**The browser is excluded from the count**: it is the same binary and the same
work in all three cases, so adding it would only contribute noise equally to all
of them.

The helpers are identified by PID against a snapshot taken just before launch: on
a development machine there are `node` processes belonging to other things — the
editor, for one — and counting those would skew the result.

## Results (2026-08-08)

Intel i7-1165G7, 24 GB RAM, Windows 11, Chrome 151, Python 3.13, Selenium 4.30,
Playwright 1.62, Orion release. **All five variants drive the same Chrome** and
return the same fingerprint `a4f7969f5377`. Best of five complete passes.

| variant | extraction | full process | process RAM | stack RAM | helper |
|---|---:|---:|---:|---:|---|
| Selenium, idiomatic | 14,132 ms | 24,953 ms | 39.0 MB | 62.3 MB | chromedriver |
| Selenium, hand-written JS | 7.7 ms | 8,088 ms | 38.6 MB | 59.5 MB | chromedriver |
| Playwright, idiomatic | 9,234 ms | 12,175 ms | 38.3 MB | 317.3 MB | node |
| Playwright, hand-written JS | 31.0 ms | 1,430 ms | 34.1 MB | 156.5 MB | node |
| **Orion `extract`** | **8 ms** | **745 ms** | **16.2 MB** | **16.2 MB** | **none** |

> **A more recent run exists.** [`BROWSER.md` §19.3](../../BROWSER.md) carries a
> 2026-08-23 measurement, taken after shadow root traversal was enabled in
> `extract`, where every one of the five variants comes out somewhat slower
> (Orion 14 ms, Selenium idiomatic 14,844 ms). The machine was not in the same
> state, which is exactly why both tables carry dates. The shape of the result is
> unchanged; use the newer table if you want the current numbers.

### What these numbers say

**Orion does not run JavaScript faster than anyone.** Its extraction (8 ms) is in
the same order as Selenium's sending JS by hand (7.7 ms); that difference fits
inside the noise and inside the millisecond resolution of Orion's clock. Anyone
expecting a "10× faster" headline in that row will not find one, and saying it
would be lying.

**The real result is the first row against the last: 14 seconds against 8
milliseconds.** That first row is how Selenium and Playwright teach it in their
own documentation — locate the elements and ask each one for its text. With 500
rows × 4 fields that is 2,000 round trips, and the price is not visible in a
ten-row example: it shows up the day the catalogue grows.

What `extract` contributes is not raw speed, it is that **the fast path is the
only path**. In the other two you have to know the problem exists, and then write
JavaScript by hand inside Python — which is exactly the work you were hoping not
to have to do.

**Launching and closing is a fundamental difference: 745 ms against Selenium's
8.1 seconds.** And it is not the extraction, which is 8 ms. Broken down:

| Selenium, where its 8 seconds come from | |
|---|---|
| start Python and import the library | ~0.4 s |
| resolve and launch `chromedriver` + Chrome | ~1.4 s |
| `driver.quit()` | ~2.1 s |
| **after the script's last line** | **~4.2 s** |

That last row was measured by comparing the program's internal clock (4.08 s)
with the process's wall time (8.23 s): the Python process does not finish exiting
until its `chromedriver` tree is completely gone. For a one-off task it does not
matter; for a job launched every five minutes, it is eight seconds per pass doing
nothing.

**In memory the difference is of another order: 16 MB against 60 and against
157.** A helper process is not free, and Playwright's is an entire JavaScript
runtime. Its idiomatic version reaches **317 MB** because it also retains a
handle for every element queried: 2,000 handles alive at once.

That last point matters more than it looks when the work runs on a server with
several tasks at a time: reserving 16 MB per process is not the same as reserving
157.

### What this benchmark does NOT measure

- **A single local page.** There is no network, no latency, and no site that is
  slow to answer. In a real scraper that usually dominates the total time, and
  there all three tools wait the same.
- **Reading only.** It does not measure clicks, forms or actionability waits.
- **Playwright loses an advantage here**: it is given the installed Chrome so the
  comparison is fair, and that hides what it contributes by bringing its own
  browser at a pinned version.
- **The browser is not counted**, neither in launch time nor in memory. It is the
  heaviest part of all and it is identical for all three.
- One machine and one operating system.

# Orion `browser` — reference

> Spanish version: [BROWSER.es.md](BROWSER.es.md) — kept for reference, but
> this English page is the canonical one and the Spanish one lags behind it.

Web automation over CDP (Chrome DevTools Protocol). No external driver, no
`chromedriver`, no new dependencies: the module uses the synchronous
`tungstenite` and the `serde_json` that Orion already carries inside.

```orion
use "browser" as web

with b = web.open() {
    p = web.page(b)
    web.goto(p, "https://example.com")
    show(web.title(p))
}
```

`with` desugars to `web.free(b)` even if the body raises an error, and `free`
closes the browser's tabs in cascade. No orphan processes are left behind.

> **Status**: transport, launch, navigation, interaction (iframes and shadow DOM
> included), forms, tables, dialogs, windows, extraction (with schema
> discovery), files, session, cookies, stability, network capture, request
> interception, device emulation and parallel crawling (`crawl`) are verified
> end to end (94 e2e tests in
> [`orion-vm/tests/browser_e2e.rs`](orion-vm/tests/browser_e2e.rs), against a
> local server). **Zero hardcoded constants**: everything that decides behaviour
> can be changed from `open()` — see 1.2. Measured against Selenium and
> Playwright in 19.3, with the methodology in
> [`bench/web/README.md`](bench/web/README.md).

## 1. Launch

### 1.1 `web.open(opts?)` → browser

It locates the browser in a cascade, **with nothing hardcoded**:

1. `opts.chrome` — an explicit path
2. `ORION_CHROME` — environment variable
3. Auto-detection: Chrome, Chromium, Brave or Edge

On Windows it matters that Edge is accepted: it ships with the system, so there
is nothing to download.

**Orion does not download any browser.** It uses the one you already have. If
there is no Chromium-based browser at all, `open()` fails saying which ones work
and how to point at the path.

What does disappear entirely is `chromedriver`: CDP talks to the browser
directly, so there is no second binary whose version has to be kept in sync.
Chrome updating itself stops being a problem.

The endpoint is discovered two ways, because not every browser offers both:
Chrome announces it on its error output *and* writes `DevToolsActivePort` into
the profile; **Edge only writes the file**.

```orion
b = web.open({
    chrome:   "C:/path/chrome.exe",  -- optional
    headless: yes,                   -- yes by default
    images:   no,                    -- NOT downloaded by default
    gpu:      no,
    width:    1280,
    height:   800,
    timeout:  30000,                 -- ms, browser launch
    user_data: "C:/profile",         -- your own profile (sessions persist)
    args:     ["--proxy-server=x:1"],   -- extra flags, appended last
    without:  ["--disable-extensions"], -- default flags to remove
    allow:    ["*.company.com"]       -- domain allowlist, see 9.2
})
```

`args` **adds** and `without` **removes**. Both are needed: in Chrome a later
flag does not always revert an earlier one, so without `without` a site that
needed extensions had no way to undo `--disable-extensions`. Flags are removed by
name, without repeating the value: `without: ["--blink-settings"]`.

**Images are off by default.** They are the bulk of a page's memory and network
use, and almost no scraper needs them. Turn them back on with `images: yes`,
which is mandatory for faithful screenshots.

Without `user_data` a temporary profile is created and deleted on close. With
`user_data` the profile is yours and is left alone: that is how you keep sessions
between runs.

### 1.2 Tuning

Nothing in the engine is hardcoded. The parameters are grouped into two levels
according to what they are:

**Policy** — decisions about *your* problem, at the root of the options:

| Option | Default | What it controls |
|---|---|---|
| `wait` | 10000 | wait for actions and reads, in ms |
| `retry` | 50 | how often it retries inside the page |
| `cdp_margin` | 5000 | margin of the transport deadline over the wait |
| `drag_steps` | 10 | intermediate steps of a drag |
| `force_layers` | 12 | stacked layers that `force` goes through |
| `iframe_depth` | 8 | depth of nested iframes that is traversed |
| `shadow` | `yes` | whether selectors enter open shadow roots |
| `shadow_depth` | 8 | depth of nested shadow roots that is traversed |
| `hit_inset` | 24 | margin in pixels when probing points of an element |
| `nav_settle` | 5000 | how long the page is tolerated changing documents |

**Mechanism** — resource usage, under `tuning` so the day-to-day API stays clean:

| Option | Default | What it controls |
|---|---|---|
| `max_events` | 512 | retained CDP events (more history is more RAM) |
| `idle_poll` | 5 | ceiling of the idle poll (raising it lowers CPU) |
| `close_timeout` | 2000 | deadline for close operations |
| `send_timeout` | 5000 | deadline for a send to make progress |
| `cleanup_tries` | 12 | attempts to delete the temporary profile |
| `stale_profile_mins` | 60 | age at which an abandoned profile is swept |

```orion
b = web.open({
    wait: 4000,
    drag_steps: 25,
    tuning: { max_events: 64, idle_poll: 20 }
})
```

`wait` has **three levels**, from the most specific to the most general: what the
call says, what was set when opening, and the default.

```orion
web.text(p, "#slow", 6000)   -- overrides the browser's wait
```

### 1.3 `web.page(browser)` → tab

### 1.4 `web.free(handle)` / `web.close(handle)`

Works for a browser or a tab; it is the name `with` invokes.

### 1.5 `web.pages(browser)` → list of handles

### 1.6 `web.info()` → diagnostic dictionary

Which browser would be used, where it comes from, and how many are open. Without
this, an "it doesn't work for me" is impossible to debug.

```orion
show(web.info())
-- {found: yes, path: C:\...\chrome.exe, env: , open_browsers: 0, in_use: [], open_pages: 0}
```

## 2. Navigation

| Function | Returns |
|---|---|
| `web.goto(tab, url)` | the final URL |
| `web.title(tab)` | title |
| `web.url(tab)` | current URL |
| `web.content(tab)` | full HTML |
| `web.reload(tab, opts?)` | reload, see 10.1 |
| `web.back(tab)` / `web.forward(tab)` | history, see 10.1 |

`goto` waits for the page to load. If it does not load because an `alert` froze
it, it says so with the dialog's text instead of a generic timeout.

## 3. Selectors

**One kind only**, deduced from the text itself. There is no `find_by_xpath` and
`find_by_css`.

| Form | Means |
|---|---|
| `.card > button` | CSS |
| `//li[@data-n='2']` | XPath (starts with `//` or `(//`) |
| `text=Buy` | by visible text |

The by-text variant exists because most XPath people write is there to search by
content, and it comes out fragile and unreadable.

**Selectors cross accessible iframes**, which is where nearly every cookie
consent dialog lives. Cross-origin iframes are skipped without breaking the
search.

### 3.1 Shadow DOM

**Selectors also enter open shadow roots**, at any depth. A web component keeps
its content in a shadow root and the document's `querySelector` does not go in
there: the correct selector "does not exist" and there is no hint as to why. Half
the modern web — from a video player to Salesforce forms — is exactly this.

```orion
-- <my-card> keeps its button in a shadow root: nothing needs to be said.
web.click(p, "#btn")
web.extract(p, ".row", { name: ".nm" })
```

Both the search **and the click** go in: the hit test descends through shadow
roots, because otherwise `elementFromPoint` returns the host, `host.contains(button)`
is false — `contains` does not cross the boundary — and every component would
look covered by itself.

Three things worth knowing:

- **Closed shadow roots** (`mode: 'closed'`) are not reachable, not even for the
  browser from outside. There is no trick: `exists` says `no`, which is the
  honest answer.
- **XPath does not cross** the shadow boundary, by definition of the standard.
  Inside a component you have to use CSS or `text=`.
- **The normal case does not pay for it.** Shadow roots are traversed after the
  documents, so an element in the ordinary DOM resolves on the first attempt.
  Measured on a 500-row page with no components: 3 ms of difference. It is turned
  off with `open({ shadow: no })`.

## 4. Interaction

| Function | Note |
|---|---|
| `web.click(tab, sel, opts?)` | |
| `web.dblclick(tab, sel, opts?)` | |
| `web.rightclick(tab, sel, opts?)` | |
| `web.hover(tab, sel, ms?)` | |
| `web.drag(tab, source, target, ms?)` | with intermediate steps, for `dragover` |
| `web.scroll(tab, dx, dy)` | mouse wheel |
| `web.type(tab, sel, text, opts?)` | clears the field unless `{ clear: no }` |
| `web.press(tab, key)` | see 4.3 |
| `web.select(tab, sel, option, ms?)` | native `<select>`, see 4.4 |
| `web.fill(tab, fields, opts?)` | a whole form in one call, see 4.5 |
| `web.check(tab, sel)` / `web.uncheck(...)` | checkboxes, see 4.6 |

The third argument accepts a number (milliseconds to wait) or a dictionary:

```orion
web.click(p, "#go", 3000)                    -- waits up to 3 s
web.click(p, "#go", { wait: 3000, force: yes })
```

### 4.1 Implicit waiting, always

No action requires remembering to add a `wait`. `click` and friends wait for the
element to be **actionable**: it exists, it takes up space, it is not hidden by
style, and nothing covers it. A scraper that depends on the programmer
remembering to wait is a scraper that fails intermittently.

The retry loop lives **inside the page**, so all of this is still a single CDP
call.

### 4.2 Covered elements

There are three situations and each has its own answer:

| Situation | Behaviour |
|---|---|
| Temporarily covered (a spinner, a banner on its way out) | waits and clicks |
| Partially covered (a sticky header over half of it) | clicks on the free area |
| Permanently covered (a cookie banner) | fails, naming the culprit |

Partial covering is solved by probing **nine points** inside the element's
rectangle (centre, four offsets, four corners) instead of just the centre, which
is what the other tools do. It is what a person would do: click where you can see
it.

When it really cannot be done, the error identifies what is in the way:

```
browser.click '#total': it is covered by <div.cookie-banner> (after waiting 1200 ms)
  If whatever is in the way will not go away, use: { force: yes }
```

`{ force: yes }` goes through. **It does not blind-click the coordinates** — that
is how Selenium ends up pressing the banner instead of the button: whatever is in
the way is made transparent to the pointer, the click is still a real browser
event, and everything is restored afterwards (even if the click fails).

It is deliberately not the default: going through a modal usually means skipping
something the site is asking you for, and that produces strange sessions that
fail three steps later.

### 4.3 Keys

`enter`, `tab`, `escape`, `backspace`, `delete`, `space`, `up`, `down`, `left`,
`right`, `home`, `end`, `pageup`, `pagedown`.

`web.type` sends **key by key**, it does not assign `value` from JavaScript:
React, Vue and company only find out about the change if the keyboard events
arrive, and a `value` set by hand is ignored when the form is submitted.

### 4.4 Native `<select>`

A `<select>` opens a dropdown **belonging to the operating system**, outside the
DOM: no click can navigate it. `web.select` assigns the option and emits `input`
and `change` the way the browser would.

It accepts the `value`, **the visible text** or the index, because whoever writes
the scraper sees the text on screen:

```orion
web.select(p, "#country", "Mexico")   -- by text
web.select(p, "#country", "mx")       -- by value
web.select(p, "#country", "1")        -- by index
```

If the option does not exist, the error lists the ones that do.

### 4.5 `web.fill(tab, fields, opts?)` → how many it filled

A whole form in **a single call**, with the type of each control decided by the
page:

```orion
web.fill(p, {
    "#name":    "Ana Torres",
    "#notes":   "long text",
    "#country": "Spain",      -- <select>: visible text, value or index all work
    "#accept":  yes,          -- checkbox
    "#plan_b":  yes,          -- radio
    "#bio":     "..."         -- contenteditable
})
```

Forcing you to choose the function based on what the field is made of — `type`
for text, `select` for the dropdown, `check` for the checkbox — means reading the
HTML before you can write a single line.

**Order is respected**, and it is needed: a province dropdown that only fills in
once you pick a country has to come after the country.

**Why it is fast.** `type` sends two CDP events per character. Measured against a
real site, 51 characters key by key cost **221 ms** and the same assignment in one
call costs **1 ms**.

**Why `type` still exists.** Real keys are needed when the site reacts to them:
autocompletes, phone masks, search boxes that filter as you type. For those,
`{ keys: yes }` puts `fill` into the slow, faithful mode, field by field.

```orion
web.fill(p, { "#search": "madr" }, { keys: yes })   -- triggers the autocomplete
```

#### The `value` trap

Assigning `el.value = x` and firing an event **does not reach the application**
if the site uses React. React installs a tracker on the element's `value`
descriptor and, when the event arrives, compares it with the last thing it
recorded: if they match, it considers the change already seen and tells nobody.

The result is the worst possible failure: **the field looks filled on screen and
the form is submitted empty.** Verified against the same mechanism React uses:

| How it is filled | Does the application find out? |
|---|---|
| `el.value = x` + event | **No** |
| native prototype setter + event | Yes |
| real keys | Yes |

`fill` writes through the native prototype setter, which the tracker does not
intercept. And it uses the correct prototype: the `HTMLInputElement` one is no
good for a `<textarea>` and the assignment would be lost without a word.

It also `blur`s after finishing each field, because many forms validate on focus
loss and otherwise the field ends up filled but marked in red, with the submit
button disabled.

#### What it cannot find, it says

A field that does not get filled is almost never missing data: it is the wrong
selector, or the form changed. Keeping quiet leaves the submission incomplete and
the failure shows up on somebody else's server.

```
browser.fill: 1 field(s) do not exist on the page:
    #old_phone
    #country  ->  no option "Mars"
  Options: Spain, Portugal
  Check those selectors, or use { strict: no } if they really may be absent.
```

It waits for **all** of them before touching any: stopping halfway through leaves
the form in a state nobody wrote.

### 4.6 `web.check(tab, sel)` / `web.uncheck(tab, sel)`

Checks or unchecks with a **real click**, and only if needed:

```orion
web.check(p, "#accept")
web.check(p, "#accept")   -- already was: does nothing
```

Idempotence is not a detail. If it just pressed, an innocent retry — or a loop
that reviews the checkbox — would leave it in the opposite state to the one
requested.

An `<input type="radio">` cannot be unchecked by pressing it, and `uncheck` says
so instead of failing silently: you have to check another one in the group.

## 5. Reading the DOM

| Function | Waits? |
|---|---|
| `web.text(tab, sel, ms?)` | yes |
| `web.texts(tab, sel, ms?)` | yes |
| `web.html(tab, sel, ms?)` | yes |
| `web.attr(tab, sel, attribute, ms?)` | yes |
| `web.value(tab, sel)` | yes — what the field holds RIGHT NOW, see 5.1 |
| `web.table(tab, sel, opts?)` | yes — a whole `<table>`, see 5.2 |
| `web.watch` + `web.capture` | the JSON the page asks its own API for, see 12 |
| `web.discover(tab, opts?)` | works out the extraction schema by itself, see 7.5 |
| `web.crawl(browser, opts)` | walks urls in parallel, dumps and resumes, see 7.6 |
| `web.exists(tab, sel)` | **no** |
| `web.count(tab, sel)` | **no** |
| `web.visible(tab, sel)` | **no** |
| `web.wait(tab, sel, ms?)` | explicit wait |
| `web.wait(tab, { idle: ms })` | waits for the network to go quiet, see 10.2 |

The rule: **what returns content waits; what reports state does not.**

Returning `null` because the content had not arrived yet turns a timing problem
into silently lost data — the failure that makes a scraper work on the laptop and
not on the server. The other way round, making `exists` wait would turn a
legitimate "it's not there" into ten seconds of blocking.

### 5.1 `web.value(tab, sel)` → what the field holds right now

```orion
web.fill(p, { "#name": "Ana" })
show(web.value(p, "#name"))          -- Ana
show(web.attr(p, "#name", "value"))  -- null
```

The two lines above do not contradict each other, and confusing them is a
classic: `attr` reads the **HTML attribute** — the one written in the page — and
that does not change when somebody types into the field. An `<input>` with no
`value=` in the HTML returns `null` that way even when it has text inside, which
is exactly the moment you believe your `fill` did not work.

`value` also returns what corresponds to each control: the value of the chosen
option in a `<select>`, `yes`/`no` in a checkbox, and the text in a
`contenteditable`.

### 5.2 `web.table(tab, sel, opts?)` → list of records

```orion
rows = web.table(p, "table.wikitable")
show(len(rows))       -- 222
show(rows[1])
-- {Country/Territory: United States, IMF (2026)[1]: 32,383,920, ...}
```

A whole table in one call, with the header deduced and the columns already named.
It chains directly into the data engine.

**The rules here come from looking at real tables, not from imagining them.** Out
of 13 tables across three Wikipedia pages:

| | How many |
|---|---|
| Without `<thead>` | **13 of 13** |
| With `<th>` inside the body (row headers) | 10 |
| With `colspan` or `rowspan` | 4 |
| With another table inside | 1 |

A reader that assumes `<thead>` — which is how the first version comes out —
works perfectly on the demo site and fails on 100% of real tables. Hence the four
decisions:

1. **The header is looked for in a cascade**: `<thead>`, or the first row if
   **all** its cells are `<th>`, or generated names `col_1`, `col_2`…
2. **Requiring that they all be `<th>`** is what avoids mistaking a data row that
   starts with a row header for the table's header. That is the case in 10 of the
   13.
3. **`colspan` and `rowspan` are expanded.** Without that, the columns fall out of
   alignment from the first merged cell onwards and everything after it is shifted
   one place, looking like good data.
4. **The rows of a nested table belong to the inner one**, not to this one.

With multi-level headers the bottom one wins, since it is the one that names
columns.

**Column names are cleaned** because they are keys: whitespace is collapsed (a
header with a `<br>` would give a key with a line break in it, and nobody can type
that), empty ones become `col_N` and repeated ones are numbered (`n`, `n_2`).
**Values are left untouched**: there a line break can be part of the data.

`{ header: no }` interprets no row as a header and returns everything as data with
generated names. It is needed for tables used as layout, where the first row is
already data.

## 6. Dialogs and windows

### 6.1 `web.dialogs(tab, policy)`

For `alert`, `confirm` and `prompt`:

```orion
web.dialogs(p, "accept")          -- accepts
web.dialogs(p, "dismiss")         -- dismisses
web.dialogs(p, "answer:Orion")    -- answers a prompt
web.dialogs(p, "off")             -- stops handling them
```

It is declared **once** and holds for the session. Playwright forces you to
register the handler *before* each action that might open one, and that fails when
the dialog is fired by a timer on the page: there is no call of yours to hook it
to.

An unhandled dialog **freezes the page** without producing any error, which is the
worst possible failure. That is why the CDP reader thread itself handles them.

### 6.2 `web.click_opens(tab, sel, opts?)` → new tab

A click that opens a tab gives you back its handle, already loaded:

```orion
invoice = web.click_opens(p, "#view-invoice")
show(web.title(invoice))
```

Playwright needs the click wrapped in `expect_popup`; Selenium makes you list
window handles and guess which one is new.

### 6.3 HTML modals

They need nothing special: they are HTML. The blocking backdrop also behaves
properly — with the modal open, a click outside fails naming the culprit instead
of slipping underneath.

## 7. Extraction

### 7.1 `web.extract(tab, row_selector, schema, opts?)` → list

The schema is a dictionary of field to specification, and **all of it compiles
into a single call** that runs inside the page.

```orion
schema = {
    id:    "@data-id",
    name:  ".title",
    price: ".price|num",
    stock: "[data-qty]@data-qty|int",
    url:   "a@href",
    avail: ".disp|bool"
}
items = web.extract(p, ".card", schema)
```

```
{id: 1, name: Laptop Pro, price: 1299,  stock: 7,  url: /p/1, avail: yes}
{id: 2, name: Mouse,      price: 24.99, stock: 0,  url: /p/2, avail: no}
```

There is the fundamental difference with Selenium: there **every attribute read
is an HTTP request to the driver**, so 500 products by 3 fields is about 1,500
round trips plus the 500 to locate the rows. This is one. And since
`returnByValue` is used, what crosses the socket is the data you asked for, not
the HTML.

`extract` waits for rows to exist before giving up: the listing usually arrives
after the action that requested it, and returning an empty list would turn a
timing problem into a silent empty result.

### 7.2 Grammar of a specification

All three parts are optional: `<selector> @<attribute> |<conversion>`

| Example | Means |
|---|---|
| `.price` | text of the element |
| `a@href` | attribute of a descendant |
| `@data-id` | attribute of the row itself |
| `.price\|num` | text converted to a number |
| `//td[2]\|num` | XPath **relative to the row** |
| `\|num` | the text of the whole row, as a number |
| `.tag\|list` | **all** the matches, not the first |
| `.p\|list:num` | all of them, converted to number |
| `a@href\|list` | every link in the row |

Conversions: `num`, `int`, `bool`, `html`, `text`, `trim`, `list`,
`list:<conversion>`.

Three details that prevent silent errors:

**`list` collects them all.** Without it, a field with several values — a
product's tags, a gallery's images — returned the first match and the rest were
lost without a word. An empty list in **every** row counts as a dead selector
just like a `null`, so the warning in 7.3 still works there, which is where it is
needed most.

Inside a list the `null` coming from a conversion is preserved (`"Sold out"` with
`list:num`), because there really was something there and you need to see it to
understand why no number came out. What gets skipped are the elements with
nothing inside.

**XPath is made relative.** `//td[1]` is absolute and would search from the root
of the document, returning **the same row repeated** with data that looks fine.
Since a field specification describes, by definition, something inside the row, it
is converted to a relative one.

**Numbers understand both formats.** `1.299,00 €` and `$1,234.56` coexist on the
same page. The separator that appears furthest to the right wins; with a lone
comma, it is decimal if it separates one or two trailing digits and thousands if
not. A non-numeric value such as `Sold out` gives `null`, not an invented number.

### 7.3 Dead selectors

A field that is empty in **every** row is almost never missing data: it is the
wrong selector, or the site that changed structure. Keeping quiet returns a list
that looks fine and blows up a hundred lines later — the classic BeautifulSoup
failure.

```
browser.extract: 2 field(s) matched nothing in any of the 3 rows:
    price  ←  .old-price
    sku  ←  @data-sku
  Check those selectors, or use { strict: no } if they really may be absent.
```

With `{ strict: no }` it is accepted and those fields come back as `null`.

### 7.4 `web.extract_to(tab, urls, selector, schema, out, opts?)` → summary

Walks several URLs and **dumps to disk as it extracts**.

```orion
r = web.extract_to(p, urls, ".card", schema, "products.csv")
show(r)
-- {rows: 8000, urls: 40, ok: 40, failed: 0, empty: [], files: [products.csv], errors: []}
```

Two deliberate decisions: **a single tab is reused** for all the URLs (opening one
per page multiplies the browser's memory) and **the listing is not accumulated**
before saving, which is what makes a Python scraper eat RAM as soon as the volume
grows.

Measured with 200 rows per page:

| Pages | Rows | Orion peak RAM |
|---|---|---|
| 5 | 1,000 | 18.4 MB |
| 40 | 8,000 | 18.9 MB |

Eight times the data, half a megabyte more. The measurement is of the Orion
process; the browser's memory is separate and it is the large one, which is why
images are off by default.

The honest limit: memory is bounded by **the largest page**, not by the total of
the walk, because each page is extracted in one go.

**Formats**, according to the extension:

- `.csv` — written row by row, a single file, genuinely constant memory.
- `.odf` — the binary format carries the row count in its header and does not
  allow appending, so it is dumped in blocks (`chunk`, 50,000 by default),
  freeing each one. The first keeps the requested name and the rest are numbered.
  `frame` reads it directly, with the types already inferred:

```orion
h = fr.open("products.odf")
show(fr.schema(h))    -- {id: int, name: string, price: float, stock: int}
```

**A URL that fails does not abort the walk.** In a batch of twenty, dying on a 404
throws away the work of the nineteen good ones: it is recorded in `errors` and the
walk continues.

**A page that loads but yields no rows is reported in `empty`.** A 404 with a
template, a redirect to the login page, or a selector that stopped working in that
section all load fine and produce nothing. Without this, a walk loses pages
silently and nobody notices until data is missing from the report.

### 7.5 `web.discover(tab, opts?)` → proposed schema

A scraper's problem is not reading data, it is **working out which selector to
use**. You open the browser's tools, walk down the tree, try a class, see that it
also matches the menu, try another one… and twenty minutes later you have a schema
that breaks on the next page.

`discover` looks at the page and proposes one:

```orion
e = web.discover(p)
show(e["row"])       -- ".quote"     (the selector of the repeating row)
show(e["fields"])    -- {text: ".text", author: ".author", url: "a@href"}
show(e["sample"])    -- the first rows already extracted with that proposal

-- and it is used as is:
rows = web.extract(p, e["row"], { quote: ".text", author: ".author" })
```

It returns `{ row, count, fields, sample, fragil }`. The **sample** is what makes
it trustworthy: it does not ask you to believe the proposal, it shows you what it
would extract.

How it works it out, so it is not magic:

- **The row** is the group of sibling elements that repeats most with the same
  internal structure, scored by count **and richness** — text and number of
  fields. That way it does not mistake a product listing for the navigation menu,
  which also repeats but is empty.
- Repetition is detected by **structure, not by classes**: modern sites generate
  classes like `x1i10hfl` that mean nothing, so the tag and the children's tags
  are what get looked at.
- **The row selector** is the class common to every row that also selects exactly
  those. If no class works, it falls back to a structural selector
  (`article > h3 > a`) and `fragil` comes back `yes` to warn about it.
- **Fields** are only kept if they appear in most of the rows: one that is in a
  single row is not a field, it is a coincidence.

It does not guess intent — it does not know that something is a "price", so a
field with no readable class is called `campo_1`. It does not replace `extract`:
it leaves you one step away from it instead of twenty minutes. Nobody ships this;
in Python you sit down and read the HTML by hand.

Measured on three different sites without being told anything about any of them:
on Hacker News it pulls out the article URL and its title; on a bookshop, the
link, the thumbnail and the price; on a quotes listing, the text and the author.

### 7.6 `web.crawl(browser, opts)` → summary

`extract_to` walks a list of URLs with **a single tab, serially**. It works, but
it leaves the machine at an eighth throttle: while one page loads — which is
waiting on the network, not computing — the rest of the browser sits idle.

`web.crawl` opens **N tabs and drives them in parallel from N system threads**:

```orion
r = web.crawl(b, {
    urls:    pages,                      -- the list of pages
    row:     ".card",
    schema:  { name: ".title", price: ".price|num" },
    out:     "catalog.csv",
    workers: 8,                          -- 8 tabs at a time
    resume:  yes                         -- picks up if it was cut off
})
show(r)
-- {rows: 4000, ok: 40, failed: 0, skipped: 0, workers: 8, empty: [], files: [catalog.csv], errors: []}
```

You pass it the **browser**, not a tab: it opens them itself. It takes `row` and
`schema` from `extract`, and writes to disk with the same streaming dumper as
`extract_to` (flat RAM, `.csv` or `.odf`).

**The parallelism is real, and it is the muscle Orion has and a Python scraper
does not**: genuine system threads over the same CDP socket, which the transport
multiplexes. It is not cooperative `asyncio`. Measured against a local server of
12 slow pages: `extract_to` serially **7.9 s**, `crawl` with 8 workers **1.8 s** —
the same 120 rows. The factor depends on how many pages and on the network; what
changes is the shape.

**It resumes.** A ten-thousand-page walk that is cut off at seven thousand cannot
start from scratch. Every finished URL is recorded in `<out>.progress`, and on
starting again with `resume: yes` the completed ones are skipped (`skipped` counts
them). It is recorded **after** its rows are written: if the process dies in
between, that page is repeated on resume instead of being lost. Resuming is for
`.csv` — which allows appending; `.odf` forces starting over and says so.

Like `extract`, a field that brings no value on **any** page gives itself away
instead of leaving an empty column that looks fine; with `{ strict: no }` it is
accepted.

In Python this is **Scrapy**: a whole framework, another settings file, another
mindset. Here it is one call, resting on pieces that already existed.

## 8. Files

The three things the browser **delegates to the operating system**: choosing a
file to upload, saving a downloaded one, and printing. All three open a native
window that is outside the DOM, so no click and no key can reach it. It is where
real web automation gets stuck.

None of those windows is handled here: **they are prevented from existing**. CDP
allows all three to be intercepted before the browser asks the system for them, so
none of this depends on the language of the Windows install, on the resolution, or
on there being a desktop at all. It works the same headless and on a server with
no screen.

### 8.1 `web.upload(tab, selector, files)` → absolute paths

```orion
web.upload(p, "#attachment", "contract.pdf")            -- one
web.upload(p, "#attachment", ["a.pdf", "b.pdf"])        -- several
```

The selector can point at two different things, and both work:

1. **The `<input type="file">` itself.** The files are assigned to it and that is
   that.
2. **Anything that opens the picker when pressed** — the "Browse" button, a
   drag-and-drop zone, a `<label>`. The real `<input>` is usually hidden behind
   the site's design and sometimes is not even reachable with a selector.

Case 2 is the one Selenium does not cover: its recipe is `send_keys` on the input,
which requires the input to exist and be reachable. Here interception is turned
on, the element is pressed, and when the browser announces it was about to open
the window it is answered with the files. The window never appears.

Relative paths are resolved against the program's directory, not the browser's,
which is another process and is somewhere else. The resolved absolute paths are
returned because without seeing them it is impossible to understand why the
browser says a file that exists does not exist.

**A file that does not exist is reported before touching the page.** The browser
silently accepts an invented path: the form is submitted with no attachment and
the failure shows up much later, on somebody else's server.

```
browser.upload: the file 'contract.pdf' does not exist
  looked in: C:\work\invoices\contract.pdf
```

### 8.2 `web.download(tab, selector, opts?)` → dict

```orion
d = web.download(p, "#download", { dir: "invoices" })
show(d)
-- {path: C:\work\invoices\invoice-042.pdf, name: invoice-042.pdf, bytes: 51234, url: https://...}
```

Downloading with an automated browser has two problems, not one:

**The "Save as" dialog**, which is avoided by fixing the download behaviour before
pressing.

**Knowing when it has finished.** The browser first writes a temporary
`.crdownload` file and renames it when done. Without a notification, the usual
recipe is to sleep a few seconds and cross your fingers: if the network is slow
you read a half-written file, and if it is fast you waste the time. Here the
completion event is awaited, so the call returns exactly when the file is whole —
and `bytes` confirms it.

| Option | What it does |
|---|---|
| `dir` | Destination folder. Created if absent. By default, the program's. |
| `name` | Renames on finish. By default, whatever the server proposes. |
| `overwrite` | Allows overwriting an existing file. **No** by default. |
| `wait` | Deadline, in ms, for large files. |

**Two downloads with the same name do not overwrite each other.** The second ends
up as `report (2).txt` and the real path comes back in `path`. Overwriting
silently is what loses a whole batch of invoices without anyone noticing until
month end; it is requested explicitly with `{ overwrite: yes }`.

**An element that does not download says so**, instead of waiting around:

```
browser.download: pressing '#view' did not start any download in 10000 ms.
  Check that the element is the one that downloads, and not a link that opens the
  file in a tab.
```

### 8.3 `web.pdf(tab, path, opts?)` → path

```orion
web.pdf(p, "receipt.pdf", { margin: 0.4, landscape: no })
```

It is not a screenshot: it is the whole document, paginated and with selectable
text. To save a receipt or an invoice from a web portal it is what you need, and
it is exactly what forces you to fight the print dialog if done by hand.

Options: `landscape`, `background`, `headers`, `scale`, `width`, `height`,
`margin`, `pages`. Measurements are in inches, which is the browser's unit — an A4
is 8.27 × 11.69. Anything not specified is decided by the browser with the same
default the dialog would apply.

The background is printed by default, unlike in the dialog: the browser removes it
to save ink, and in a PDF nobody is going to print that only makes tables with
alternating rows come out blank.

## 9. Session and security

### 9.1 `web.save_state(tab, path)` / `web.load_state(tab, path)`

The most expensive part of an automation that runs daily is not navigating: it is
**logging in again on every run**. It is slow, and above all it is fragile — every
login is a form that can change, a captcha that can appear and a second factor
that can fire. A process that logs in a hundred times a day is also a process that
looks like an attack.

```orion
-- once
web.save_state(p, "session.json")

-- every day
web.goto(p, "https://portal.company.com")
web.load_state(p, "session.json")
web.reload(p)                        -- already inside
```

`save_state` returns what it saved and `load_state` what it applied:

```
{path: session.json, cookies: 5, local: 3, session: 0, origin: https://portal.company.com}
{cookies: 5, local: 3, session: 0, skipped: []}
```

**You have to be on the origin before restoring.** Cookies go to the whole
browser, but local storage can only be written while on its domain — the browser
does not allow touching another's. Origins that do not match come back in
`skipped` instead of being lost silently, because a half-restored session gives no
error at all and is impossible to debug.

`user_data` in `open()` solves something similar by saving the whole profile, but
that is a folder of hundreds of megabytes tied to one machine. This is a JSON you
can move, version separately or keep in a secrets manager.

> **This file is a credential.** The session cookies are inside it: whoever has it
> gets in as you, with no password and no second factor. It does not go in the
> repository. It is worth exactly as much as the password, with the aggravating
> factor that it does not expire when you change it.

### 9.2 `open({ allow: [...] })` — domain allowlist

```orion
b = web.open({ allow: ["*.company.com", "cdn.provider.net"] })
```

An automated process carries the company's session with it. If the page it visits
is compromised — or if an ad injected into it redirects — the bot goes elsewhere
**wearing that session**. The allowlist bounds where it can go: what is not on it
is not loaded.

`*.company.com` covers the subdomains and the bare domain; without a wildcard it
is only that exact host. The port does not count, and neither does what comes
before an at sign: `http://company.com@evil.net/` is a request to **evil.net**,
and that impersonation trick does not pass the list.

`web.blocked(browser)` returns what has been cut off, which is the first thing you
need when a site stops working with the list in place.

Interception is only activated if there is a list: with it, the browser stops on
every request and waits for an answer, and that is not paid for unless asked.

### 9.3 Credentials out of the logs

```orion
web.fill(p, { "#user": u, "#password": c }, { secret: ["#password"] })
```

A `fill` error can repeat the value that was not accepted, and that error ends up
in a log or on a shared console. Marked fields do not report theirs.
`{ secret: yes }` covers every field in the call.

## 10. Stability

### 10.1 `web.reload(tab, opts?)` / `web.back` / `web.forward`

They return the URL they end up on. `{ cache: no }` in `reload` forces fetching
everything from the server.

None of them waits for the browser's load event, and that is deliberate: **on
going back, Chrome usually restores the page from its back/forward cache without
reloading, and then there is no load event**. Waiting for it left every `back`
stuck for the entire deadline — thirty seconds — only to carry on anyway. The page
is asked instead, since it is the one that knows where it is.

A `back` with no history says so instead of doing nothing. Watch out for one
confusing detail: every tab starts at `about:blank`, so after a single navigation
there **is** a page to go back to.

### 10.2 `web.wait(tab, { idle: ms })`

Waits for the network to go quiet, for what no selector can solve: you do not know
**what** is going to appear, only that the page is still fetching things. It is
the case of a dashboard assembled from three chained calls, or a listing that
reloads when filtered.

```orion
web.click(p, "#filter")
web.wait(p, { idle: 500 })      -- half a second with no requests
```

The alternative everyone uses is to sleep two seconds, and it has both defects at
once: if the network is slow you read half of it, and if it is fast you throw away
two seconds on every pass.

In-flight requests are counted **inside the page**, by wrapping `fetch` and
`XMLHttpRequest`. That way it is a single call and does not depend on the event
history — which is bounded — having kept the ones that mattered.

The honest limit: there are pages that poll the server forever and never go quiet.
On those, the error says so and you have to wait on a selector.

## 11. Screenshots

`web.screenshot(tab, path)` → writes a PNG and returns the path.

Requires `images: yes` in `open` if you want the images to show up.

## 12. Network capture

Almost every modern site paints its listings with JavaScript from a JSON it
downloads itself. A classic scraper waits for that JSON to become HTML and then
**undoes the work**: it looks for `div`s, strips tags, rebuilds numbers that were
already numbers. And it breaks the day somebody renames a CSS class.

`watch` + `capture` read the source.

```orion
web.watch(p, "/api/products")      -- 1. arm the listener
web.click(p, "#load")              -- 2. trigger the request
r = web.capture(p)                 -- 3. collect, already parsed
```

They are two calls and not one because you have to arm **beforehand**: if the
listener switched on at collection time, the request would already have gone by
and there would be nothing left to read.

### 12.1 What you gain

In the tests' example, the page paints each product's name. Its API returns this:

```
{id: 1, name: Keyboard, price: 49.9, stock: 12, margin: 0.31, supplier: ACME}
```

`stock`, `margin` and `supplier` **never reach the HTML**. There is no selector
that can get them, because they are not there. And the ones that do arrive come
already typed: `49.9` is a number, not `"49,90 EUR"` to be converted.

| | from the HTML | from the API |
|---|---|---|
| Fields | the ones the design shows | all of them |
| Types | text, to be converted | already typed |
| Breaks when… | a CSS class changes | the API contract changes |

The second happens far less: a CSS class is touched by any redesign, and an API's
contract is defended by the site's own team.

### 12.2 `web.watch(tab, pattern)`

Without `*`, the pattern means "contains" — which is what you almost always want
and what you write first:

```orion
web.watch(p, "/api/")
```

With `*`, it is a wildcard covering any chunk, for when you need to narrow it:

```orion
web.watch(p, "*/v2/orders?*")
web.watch(p, "*.json")
```

They are deliberately not regular expressions: a URL carries `?`, `.` and `+`,
which mean something else in a regex, and the obvious pattern would give
surprising results. Here those signs are literals.

The browser's network domain is only switched on when `watch` is called: with it
on, several events are emitted per request, and a page with a hundred resources
would be hundreds of messages nobody is going to consume.

### 12.3 `web.capture(tab, opts?)` → list

Each element is `{url, status, json}`. If the body was not JSON, `json` comes back
null and the raw text goes in `text`.

```orion
r = web.capture(p)
for resp in r {
    show(resp["url"] + " -> " + str(len(resp["json"]["items"])))
}
```

**It waits for something matching to arrive** instead of looking once and coming
back empty. The request goes out after the action that triggers it, and an empty
list would turn a timing problem into "this site does not use an API" — a false
conclusion and a hard one to undo. If nothing really matches, it returns empty
once the deadline runs out.

**All** matching responses are collected, not the first: a dashboard usually asks
for three or four things at once, and keeping one would give an incomplete result
that looks complete.

**The body may have disappeared.** It does not travel in the event: it is
requested separately, and the browser keeps it in a buffer it eventually recycles.
If that happens, that element carries an `error` explaining it instead of throwing
away the whole capture. It is raised with:

```orion
b = web.open({ tuning: { body_buffer: 52428800 } })
```

### 12.4 Compared

Playwright has `page.on("response")`: a callback where you have to filter by hand,
request the body with another `await` and remember that it might not be there.
Selenium has nothing equivalent without putting a proxy in front.

## 13. Interception: deciding what the browser does with each request

`watch`/`capture` **look at** the network. `route` **decides** it. It is the
difference between observing a problem and being able to cause one.

```orion
web.route(p, "*/api/stock*", { mock: { status: 500, json: { "error": "down" } } })
web.route(p, "*.png",        { block: yes })
web.route(p, "*/api/*",      { headers: { Authorization: "Bearer " + token } })
web.route(p, "*/slow*",      { fail: "timedout" })
```

The pattern is the same as `watch`'s: without `*` it is "contains", with `*` it is
a wildcard. Never a regular expression, because a URL carries `?`, `.` and `+`.

### 13.1 What it is for

| Situation | Without `route` | With `route` |
|---|---|---|
| Test what the site does if the API returns 500 | touch the real server | one line |
| The front end is there and the back end is not | wait | `mock` |
| A listing with images and three trackers | everything is downloaded | `block`, and the whole job gets lighter |
| Authenticate where there is no form | fake a login | `headers` |
| Test a retry | not possible | `{ times: 1 }` |

### 13.2 The four actions

| Action | What it does |
|---|---|
| `{ block: yes }` | cuts the request off, like an ad blocker |
| `{ fail: "timedout" }` | cuts it off with a specific network reason, to test the error path |
| `{ mock: { status, json\|body, headers } }` | answers from Orion; the request never goes out |
| `{ headers: {...} }` | lets it through with those headers added or rewritten |

One action per rule. `{ block: yes, mock: {...} }` means nothing, and accepting it
would force inventing a precedence nobody would remember.

With `json:` it is serialized **and the `Content-Type` is set too**: without it the
page receives the correct text and its `response.json()` fails, which is a wasted
while chasing a bug that is not where it seems.

The `fail` reasons are the browser's: `failed`, `aborted`, `timedout`,
`accessdenied`, `connectionclosed`, `connectionreset`, `connectionrefused`,
`connectionaborted`, `connectionfailed`, `namenotresolved`,
`internetdisconnected`, `addressunreachable`, `blockedbyclient`,
`blockedbyresponse`. They are written however you like — `timedout`, `TimedOut`,
`timed_out` — and an invented one lists the valid ones.

### 13.3 Order rules, as in a firewall

Rules are tried in order and **the first that matches** decides. That way a
specific rule can come before a general one without the evaluation order being a
mystery:

```orion
web.route(p, "*/api/products*", { mock: { status: 200, json: data } })
web.route(p, "*/api/*",         { fail: "timedout" })   -- everything else
```

### 13.4 `{ times: n }` — failing only the first few times

```orion
web.route(p, "*/api/*", { mock: { status: 503, body: "no" } }, { times: 1 })
```

The first request gets the 503 and the second goes out for real. It is exactly
what you need to check that a retry retries, and there is no way to test that
against the real server.

### 13.5 `web.unroute(tab, pattern?)` and `web.routes(tab)`

`unroute` with no pattern removes them all and returns how many it removed.
`routes` returns what is in place and **how many times each rule has fired**,
which is how you find out that a rule you thought was active is not matching
anything:

```orion
show(web.routes(p))
-- [{pattern: */api/*, hits: 3, times: null}]
```

### 13.6 The allowlist overrides the rules

`open({ allow: [...] })` is checked **before** the routes, and a `mock` cannot
reopen a domain that was closed on purpose. It is a security measure; a
convenience rule must not be able to lift it.

## 14. Emulation: what device, language and place the browser believes it is

```orion
web.emulate(p, { device: "iphone" })
web.emulate(p, { width: 1920, height: 1080, locale: "es-ES", timezone: "Europe/Madrid" })
web.emulate(p, { dark: yes, geo: { lat: 40.4168, lon: -3.7038 } })
web.emulate(p, no)                       -- undoes everything
```

Without this there are sites that simply cannot be automated:

- **The ones that serve different HTML to mobile.** The menu you need to press
  does not exist in the desktop version: the correct selector "does not appear"
  and there is no way to make it appear.
- **The ones that depend on the time zone.** A dashboard showing "today" changes
  its data depending on where the browser thinks it is. Reproducing, from a
  machine on UTC, the bug a colleague sees in Madrid is impossible without
  pinning it.
- **The ones that change with the language.** `text=Buy` against a site that
  decided to serve English because of the CI container's `Accept-Language`.
- **The ones that ask for location.** The dialog blocks the flow and cannot be
  clicked from JavaScript.

### 14.1 Presets

`iphone`, `iphone-se`, `ipad`, `android`, `laptop`, `desktop`. They are written
however you like (`iPhone SE`, `iphone_se`) and an invented one lists the
available ones.

A preset is **a starting point, not a closed list**: any of its fields can be
overridden in the same call.

```orion
web.emulate(p, { device: "iphone", width: 1000 })   -- mobile, but wider
```

### 14.2 Every field

| Field | What it controls |
|---|---|
| `device` | starting preset |
| `width` / `height` / `scale` | dimensions and screen density |
| `mobile` / `touch` | whether the site sees a mobile and whether there are touch events |
| `ua` | User-Agent |
| `locale` | language; travels in `Accept-Language` too |
| `timezone` | IANA time zone (`Europe/Madrid`) |
| `dark` | `prefers-color-scheme` |
| `geo` | `{ lat, lon, accuracy? }` |
| `permissions` | permissions granted in advance |

**What is not asked for is not touched.** Changing the time zone does not resize
the window.

**Emulate before navigating.** Some things — touch above all — are read by the
page as it loads: `emulate` and then `goto`.

**A width with no height** is completed with the one the tab already has: CDP
accepts half-given dimensions and leaves the window in a state the site does not
understand.

**A page with no `<meta name="viewport">`** is laid out at 980 px even if you
emulate a mobile. It is not a bug: it is what the browser does, and the same
thing you would see in its developer tools.

### 14.3 Permissions

The permission dialog is a genuine blocker: it appears on top of the page and
cannot be clicked. Granting it in advance means it never exists.

```orion
web.emulate(p, { permissions: ["clipboard", "notifications"] })
```

It accepts `geolocation`, `notifications`, `camera`, `microphone`, `clipboard`,
`midi`, `sensors`, `background`.

Setting `geo` grants `geolocation` **on its own**: without it the page would
receive `PERMISSION_DENIED` and the emulated position would never actually be
used, which is the hardest failure of all of this to understand.

## 15. Cookies

`save_state`/`load_state` (9.1) move the whole session. For a single cookie:

| Function | What it does |
|---|---|
| `web.cookies(tab, name?)` | the visible cookies, or just the named one |
| `web.set_cookie(tab, cookie)` | `{ name, value, domain?, path?, expires?, http_only?, secure?, same_site? }` |
| `web.clear_cookies(tab)` | deletes every cookie in the browser |

Without `domain` the cookie is tied to the tab's url. That is needed: with
neither domain nor url the browser discards it **silently**.

## 16. JavaScript

`web.eval(tab, js)` evaluates and returns the value already converted to Orion.

```orion
n = web.eval(p, "document.querySelectorAll('.card').length")
```

A JavaScript exception becomes an Orion error, not a silent `null`.

## 17. Memory

Decisions taken with consumption as the criterion, not as a consequence:

- **The DOM never crosses the socket.** Every evaluation uses `returnByValue`: the
  requested value comes back, not a reference and not the HTML. BeautifulSoup
  brings the whole page into the process and builds a tree of objects on top of
  it; here memory is proportional to the data you asked for, not to the weight of
  the page.
- **One call per query.** In Selenium every attribute read is an HTTP request to
  the driver. All of Orion's reading is resolved inside the page, in one
  evaluation.
- **Event history bounded** to 512. An active browser emits thousands per minute
  and nobody consumes them; without a cap, a long session eats RAM on a useless
  history.
- **Images off by default**, plus the flags that turn off sync, extensions and
  background networking.
- **Tabs are really closed** on `free`: that is what releases the render process's
  memory.

## 18. Architecture

```
orion-vm/src/modules/browser/
├── mod.rs      public API and handle registry
├── cdp.rs      transport: WebSocket, multiplexing by id, event bus
├── dom.rs      selectors, waiting and actionability
├── input.rs    mouse and keyboard through CDP's Input domain
└── launch.rs   locating and launching the browser
```

Over a single socket travel responses (which carry an `id`) and events (which
carry a `method`), mixed together. One reader thread per connection hands each
response to whoever is waiting for it, sleeping on a `Condvar` — the same parking
`await` uses in `task_pool`, without introducing a second concurrency model.

Mouse and keyboard events are dispatched through CDP's `Input` domain, which
injects them into the same layer the user's own come in through. And the position
is measured again **immediately before** each dispatch, not at the start of a
chain of actions: that is the practical difference with `ActionChains`, which
between locating and clicking lets the page move the element.

## 19. Deployment

### 19.1 What you ship

```powershell
orion --build app.orx -o app.exe
```

**A single file.** `--build` does not bundle the interpreter alongside: it
compiles your program to native code with Cranelift and links it against the
Orion runtime as a static library. The result is not a launcher looking for
`orion.exe`, it is a real executable with the runtime inside.

Your user receives `app.exe` and does not need to know Orion exists.

#### What exactly was tested

A program using `upload`, `fill`, `table`, `extract`, `save_state`, `pdf` and
`reload` — that is, the whole module, not a "hello world" — compiled to **native
AOT** (61 MB) and run in a folder containing **only `app.exe`**, with no
`orion.exe` anywhere near and with `PATH` reduced to `system32`. All ten results
correct and exit code 0.

It is worth saying how it came to be right, because until 8 August 2026 this
**did not work** and the documentation said it did. Two defects, both exclusive to
the compiled executable (`orion run` was never affected):

1. **A function could not see global variables.** The compiler gave each function
   only local variables, so a global read inside one arrived as `null`. And since
   `use "browser"` defines a global, any call to the module inside a function died
   with an error about `CallMethod` that did not point at the cause. Worse still:
   a computation with a global constant gave **a different result** without
   warning.
2. **Calling your function `main`** made its symbol clash with the executable's C
   `main`, and compilation fell back to embedded bytecode. It still worked, but no
   real application — which is how they are written — ever got natively compiled.

Both now have regression tests in
[`orion-vm/tests/aot_native.rs`](orion-vm/tests/aot_native.rs), which is what was
missing: the previous suite only tested self-contained programs — arithmetic,
recursion, shapes, strings — and that is why nobody noticed.

**The lesson for reading this page**: if it says "verified" here, it should also
say *with what program*. A compiled "hello world" does not prove your application
compiles.

### 19.2 What the user's machine needs

**A Chromium-based browser, and nothing else.** On Windows it is already there:
Edge comes with the system. If their installation is in an unusual path, it is
solved without recompiling via the `ORION_CHROME` variable or by passing `chrome:`
to `open()`.

### 19.3 Compared with Python

| | Python + Selenium | Orion |
|---|---|---|
| `chromedriver.exe` | has to be shipped, and of the right version | **does not exist** |
| Runtime | Python installed, or PyInstaller | inside the `.exe` |
| Dependencies | selenium + webdriver-manager + transitive | none |
| Files to ship | a folder or an installer | **one** |
| When Chrome updates | downgrade the driver, repackage, redistribute | **nothing** |

The last row is the one that costs most in practice: in Python every Chrome update
forces repackaging. Here the executable you shipped six months ago still works.

#### Measured

500 cards × 4 fields, all three tools driving **the same Chrome**, headless,
against the same local file, and checking that all three return the same data
fingerprint. Reproducible with `bench\web\run_web.ps1`; methodology and caveats in
[`bench/web/README.md`](bench/web/README.md).

| variant | extraction | full process | stack RAM | helper |
|---|---:|---:|---:|---|
| Selenium, idiomatic | 14,844 ms | 27,028 ms | 62.3 MB | chromedriver |
| Selenium, hand-written JS | 10.4 ms | 8,188 ms | 61.7 MB | chromedriver |
| Playwright, idiomatic | 13,200 ms | 12,407 ms | 318.2 MB | node |
| Playwright, hand-written JS | 29.6 ms | 1,729 ms | 156.8 MB | node |
| **Orion `extract`** | **14 ms** | **858 ms** | **16.8 MB** | **none** |

Measured on 2026-08-23, with shadow root traversal enabled (which is what
`extract` ships with from that date). Isolated on this same page: 17 ms with
shadow and 15 ms without, so that traversal accounts for ~2 ms. The figures from
an earlier measurement were somewhat better across all five variants — the machine
was not in the same state, and that is why the table carries a date.

The RAM is that of the automation process **plus the helper it launches**, which is
not the browser and is not the same for all three: Selenium needs
`chromedriver.exe` and Playwright a `node.exe` because its driver is written in
JavaScript. Orion needs neither — it speaks CDP from its own process, which is the
same reason there is no second binary to keep in sync with Chrome's version. The
browser is excluded from the count: it is identical for all three.

Two honest readings of this table:

**Orion does not run JavaScript faster than anyone.** Its 14 ms are in the same
order as Selenium's 10.4 ms sending JavaScript by hand, and that difference fits
inside the noise. That is not the result.

**The result is the first row against the last: 15 seconds against 14
milliseconds.** That first row is how both documentations teach it — locate the
elements and ask each one for its text, which with 500 rows × 4 fields is 2,000
round trips. What `extract` contributes is not raw speed: it is that **the fast
path is the only path**. In the other two you have to know the problem exists and
write JavaScript by hand inside Python, which is exactly the work you were hoping
not to have to do.

Of Selenium's 8 seconds, the extraction is 10 ms: the rest is launching
`chromedriver` (~1.4 s), `quit()` (~2.1 s) and **~4.2 s after the script's last
line**, waiting for its process tree to finish leaving. For a one-off task it does
not matter; for a job that runs every five minutes, it does.

In memory the difference is of another order: **17 MB against 62 and against
157**. Playwright's idiomatic version reaches 318 MB because it retains a handle
per element queried, and here that is 2,000 alive at once. That weighs when the
job runs on a server with several tasks in parallel.

### 19.4 Corporate networks

This is the scenario where the difference stops being convenience and becomes
"can I or can't I".

`webdriver-manager` **downloads chromedriver** from Google domains at runtime. On
a corporate network that collides with three things at once: egress is usually
blocked (and security does not whitelist downloading executables), PyPI is closed
or behind an internal mirror, and the problem repeats with every update IT pushes.

The `browser` module **does not make a single network call of its own**. The only
thing it opens is a WebSocket to `127.0.0.1`. Verified with a scraper against a
local intranet with no internet access at any point.

Concrete advantages:

- **It works with no egress** except towards the site you are automating.
- **It uses the browser the company already administers** — Edge on a corporate
  Windows is installed and managed by policy, nothing needs approving.
- **Deterministic CI**: the "download the driver" step disappears, a classic
  source of intermittent failures unrelated to your code.
- **One single thing to audit**: one binary, instead of a dependency tree resolved
  at install time.

The corporate proxy is specified as it is for any other tool, and it reaches the
browser:

```orion
web.open({ args: ["--proxy-server=http://proxy.company:8080"] })
```

### 19.5 Worth knowing

**Size.** The executable is around 58 MB. It is Orion's complete binary: it
carries the GUI, the TUI, three database engines, OCR with its models… everything,
used or not. Today there is no way to slim it down.

**C runtime.** The binary links the MSVC CRT dynamically, so it depends on
`vcruntime140.dll`, present on any modern Windows. Compiling with the static CRT
has not been tested.

**Try it on a clean machine** before shipping it. Isolating `PATH` rules out the
important things, but a freshly installed Windows with no development tools is the
definitive check and costs five minutes.

## 20. Diagnostics

| Symptom | What to look at |
|---|---|
| "no browser was found" | `web.info()`, or set `ORION_CHROME` |
| "it is covered by `<...>`" | close that element first, or `{ force: yes }` |
| "did not appear within N ms" | is the selector right? is it in a cross-origin iframe? |
| the page freezes | `web.dialogs(p, "accept")` before the action |
| `text` returns empty | are you using `count`/`exists`, which do not wait? |

# Roadmap

Planned and designed work. What is known to be broken or missing is in
[BACKLOG.md](../BACKLOG.md), and what already shipped in
[CHANGELOG.md](../CHANGELOG.md).

## Standard library blocks

All five blocks are complete. The order in which they were built:

```
Block D ✅ → Block B ✅ → Block C ✅ → Block A ✅ → Block E ✅
 (base)       (web)        (AI)        (table/df)    (cloud)
```

## Excel and automation

> Orion does not copy pandas or openpyxl. Each feature has its own name, a
> cleaner API, and works with `|>`.

### Current state of the `excel` module

```orion
use "excel" as excel

-- What already works today
data  = excel.read("sales.xlsx")
data  = excel.filter(data, "active", "==", yes)
data  = excel.group(data, "region", { "sales": "sum", "count": yes })
data  = excel.sort(data, "region")          -- single column
data  = excel.join(data, targets, "region") -- single key
stats = excel.stats(data, "sales")
excel.write_styled("report.xlsx", data, { titulo: "Q1", stripe: yes })

-- Data plus a chart in one file, in a single call
excel.write_styled("report.xlsx", data, {
    titulo:  "Q1 Sales Report",
    stripe:  yes,
    freeze:  yes,
    charts: [
        {
            type:        "bars",
            x:           "region",
            y:           "sales_sum",
            title:       "Sales by Region",
            palette:     "orion",
            style:       "minimal",
            show_values: yes,
            sheet:       "Chart"
        }
    ]
})
```

### The nine designed features

Status below reflects what the compiler actually exposes, checked against
`orion --builtins-json`.

| # | Feature | Pandas equivalent | Status |
|---|---|---|---|
| 1 | `compute` | `df["col"].apply(fn)` | Designed, not implemented |
| 2 | `sort`, multi-column | `sort_values(["a","b"])` | ✅ Complete |
| 3 | `group`, multi-agg | `groupby().agg({...})` | ✅ Complete |
| 4 | `long` | `df.melt(...)` | ✅ Complete |
| 5 | `dates` + `date_parts` | `pd.to_datetime(...)` | ✅ Complete |
| 6 | `join`, multi-key | `merge(on=["a","b"])` | ✅ Complete |
| 7 | `chart` | openpyxl charts | ✅ Complete |
| 8 | `formula` | `ws["A1"] = "=SUM(...)"` | Partial: the `excel.f` builder exists, `excel.formula` does not |
| 9 | `sheet` builder | openpyxl cell-level | ✅ Complete |

The sections below marked as not implemented describe the intended API, not
current behaviour.

---

### F-1 `compute` - computed columns

The lambda receives the whole row, so fields can reference each other. Several
columns in a single pass.

```orion
-- Two lambda forms exist: `params => body`, whose body may be an expression
-- or a block, and `fn(params) { block }`. They do not mix: `fn row => ...`
-- is a syntax error. `if` is a statement, not an expression, so a branching
-- body needs a block with `return`.
data = excel.compute(data, {
    "bonus":    row => row["sales"] * 0.05,
    "tier":     row => {
        if row["sales"] > 90000 { return "A" }
        if row["sales"] > 70000 { return "B" }
        return "C"
    },
    "on_track": row => row["sales"] >= row["target"]
})
```

---

### F-2 `sort` - multiple columns

```orion
-- Explicit style
data = excel.sort(data, [
    { by: "region", dir: "asc" },
    { by: "sales",  dir: "desc" }
])

-- Short Orion style: + is ascending, - is descending
data = excel.sort(data, "region+", "sales-", "name+")
```

---

### F-3 `group` - several aggregations per field

```orion
by_region = excel.group(data, "region", {
    "sales":  ["sum", "avg", "max", "min"],
    "months": ["avg"],
    "count":  yes
})
-- Produces: sales_sum, sales_avg, sales_max, sales_min, months_avg, count
```

Available functions: `sum` `avg` `max` `min` `count` `first` `last` `std` `median`

---

### F-4 `long` - wide to long (unpivot)

Turns wide format into long format. A clear name: `long`, not `melt`.

```orion
-- Before (wide): region | CRM Pro | Analytics | Cloud
-- After (long):  region | product | sales

-- excel.long(data, keep, var, val) - positional, like every module function
long_data = excel.long(wide_data, ["region", "seller"], "product", "sales")
```

---

### F-5 `dates` and `date_parts`

Integrated with the `datetime` module.

```orion
data = excel.dates(data, "sale_date", "DD/MM/YYYY")
data = excel.date_parts(data, "sale_date", ["year", "month", "quarter", "weekday"])
data = excel.group(data, "quarter", { "sales": ["sum", "avg"] })
```

Formats: `"DD/MM/YYYY"` `"MM/DD/YYYY"` `"YYYY-MM-DD"` `"auto"`

Parts: `"year"` `"month"` `"day"` `"quarter"` `"weekday"` `"week"` `"hour"`

---

### F-6 `join` - multiple keys

```orion
-- Single key (unchanged)
data = excel.join(sellers, targets, "region", "left")

-- Multiple keys
data = excel.join(sellers, targets, ["region", "product"], "left")
```

---

### F-7 `chart` - declarative charts in Excel

No intermediate objects, no manual series. One call.

```orion
excel.chart("report.xlsx", by_region, {
    type:  "bars",
    x:     "region",
    y:     "sales_sum",
    title: "Sales by Region Q1",
    sheet: "Charts"
})

-- Multiple series
excel.chart("report.xlsx", by_month, {
    type:  "lines",
    x:     "month",
    y:     ["sales_sum", "target_sum"],
    title: "Sales vs Target"
})
```

Types: `"bars"` `"stacked_bars"` `"lines"` `"area"` `"pie"` `"scatter"`

---

### F-8 `formula` - live formulas in Excel

Orion does not expose raw Excel formula strings. Instead there is a builder with
clear names. Columns marked as formulas stay live in the file and recalculate
when opened in Excel.

```orion
f = excel.f

excel.write_styled("report.xlsx", data, {
    formulas: {
        "bonus":   f.pct("sales", 5),
        "total":   f.sum("sales"),
        "rank":    f.rank("sales", "desc"),
        "ratio":   f.ratio("sales", "target")
    }
})
```

Functions: `f.sum` `f.avg` `f.pct` `f.ratio` `f.rank` `f.cumulative` `f.if_`

---

### F-9 `sheet` - full cell-by-cell control

A declarative builder. No manual cell iteration.

```orion
sheet = excel.sheet("Sales Report")

sheet.put("A1", "Q1 2026 - Sales Report", { bold: yes, size: 16, merge: "A1:F1" })
sheet.put("A2", "Generated: " + datetime.today(), { color: "#888888" })
sheet.data("A4", sellers, { header: yes, stripe: yes })
sheet.chart("H4", { type: "bars", x: "region", y: "sales", width: 400, height: 300 })
sheet.style("A4:F4", { bg: "#1B4F72", color: "#FFFFFF", bold: yes })
sheet.freeze("A5")
sheet.autofilter("A4:F4")

excel.save(sheet, "custom_report.xlsx")
```

---

### The full pipeline, in a single API

`excel.compute` is not implemented yet, so it is left out of this example.
Each step rebinds `data`; the same chain can be written with `|>`, since every
`excel` function takes the table as its first argument.

```orion
use "excel" as excel

data = excel.read("sales_q1.xlsx")
data = excel.filter(data, "active", "==", yes)
data = excel.dates(data, "sale_date", "DD/MM/YYYY")
data = excel.date_parts(data, "sale_date", ["month", "quarter"])
by_quarter = excel.group(data, "quarter", { "sales": ["sum", "avg"], "count": yes })
by_quarter = excel.sort(by_quarter, "quarter+")

excel.write_styled("q1_report.xlsx", by_quarter, {
    title:      "Q1 Sales Analysis",
    stripe:     yes,
    freeze:     yes,
    autofilter: yes
})

excel.chart("q1_report.xlsx", by_quarter, {
    type:  "bars",
    x:     "quarter",
    y:     "sales_sum",
    title: "Sales by Quarter"
})
```

### Implementation order

| # | Feature | Impact | Estimated time |
|---|---|---|---|
| 1 | `compute` | Very high | 2-3h |
| 2 | `sort`, multi-column | High | 1-2h |
| 3 | `group`, multi-agg | High | 3-4h |
| 4 | `join`, multi-key | Medium | 1-2h |
| 5 | `dates` + `date_parts` | High | 3-4h |
| 6 | `long` | Medium | 2-3h |
| 7 | `chart` | Very high | 4-6h |
| 8 | `formula` | Medium | 3-4h |
| 9 | `sheet` builder | High | 6-8h |

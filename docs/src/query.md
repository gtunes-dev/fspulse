# Query Syntax

FsPulse provides a flexible, SQL-like query language for exploring scan results. This language supports filtering, custom column selection, ordering, and limiting the number of results.

---

## Query Structure

Each query begins with one of the four supported domains:

- `roots`
- `scans`
- `items`
- `changes`

You can then add any of the following optional clauses:

```text
DOMAIN [WHERE ...] [SHOW ...] [ORDER BY ...] [LIMIT ...]
```

---

## Column Availability by Domain

The table below shows which columns are available in each domain:

| Property         | Type             | Roots | Scans | Items | Changes |
|------------------|------------------|:-----:|:-----:|:-----:|:-------:|
| `root_id`        | Integer          |  ✓   |  ✓   |  ✓   |   ✓    |
| `scan_id`        | Integer          |   –   |  ✓   |  ✓   |   ✓    |
| `item_id`        | Integer          |   –   |   –  |  ✓   |   ✓    |
| `change_id`      | Integer          |   –   |   –  |   –  |   ✓    |
| `item_path`      | Path             |   –   |   –  |  ✓   |   ✓    |
| `root_path`      | Path             |  ✓   |  ✓   |  ✓   |   ✓    |
| `file_size`      | Integer          |   –   |   –  |  ✓   |   ✓    |
| `file_hash`      | String           |   –   |   –  |  ✓   |   ✓    |
| `val`, `val_old`, `val_new` | Validation Status | – | –   |  ✓   |   ✓    |
| `val_error`, `val_error_old`, `val_error_new` | String | – | – | ✓ | ✓ |
| `mod_date`       | Date             |   –   |   –  |  ✓   |   ✓    |
| `mod_date_old`, `mod_date_new` | Date | – | –  | – | ✓ |
| `hashing`        | Boolean          |   –   |  ✓   |   –  |   –    |
| `validating`     | Boolean          |   –   |  ✓   |   –  |   –    |
| `item_type`      | Item Type Enum   |   –   |   –  |  ✓   |   ✓    |
| `change_type`    | Change Type Enum |   –   |   –  |   –  |   ✓    |
| `meta_change`    | Boolean          |   –   |   –  |   –  |   ✓    |
| `scan_time`      | Date             |   –   |  ✓   |   –  |   –    |
| `adds`, `modifies`, `deletes` | Integer | – | ✓ | – | – |

---

## The `WHERE` Clause

The `WHERE` clause filters results using one or more conditions, each written as:

```text
column_name:(value1, value2, ...)
```

Each value must be valid for the column's type:

- **Integer**: numbers, ranges (e.g., `1..5`)
- **Date**: `YYYY-MM-DD`, ranges, `null`, `not null`
- **Boolean**: `true`, `false`, `T`, `F`, `null`, `not null`
- **String/Path**: quoted strings
- **Enums**: e.g., `V`, `I`, `A`, `D`, `M` depending on type

---

## The `SHOW` Clause

Controls which columns are shown and how they’re formatted. If omitted, a default column set is used.

You may specify:

- A list of column names
- The keyword `default` to insert the default set
- The keyword `all` to show all available columns

Some columns support formatting via `@` modifiers:

```text
item_path@name, mod_date@short
```

### Format Specifiers

| Type             | Format Modes                                   |
|------------------|-------------------------------------------------|
| Date             | `full`, `short`, `nodisplay`                    |
| Path             | `full`, `relative`, `short`, `name`, `nodisplay`|
| Validation/Item/Change/Boolean | `full`, `short`, `nodisplay`   |
| Integer/String   | (no formatting options)                         |

---

## The `ORDER BY` Clause

Specifies sort order for the results:

```text
items order by mod_date desc, item_path asc
```

If direction is omitted, `ASC` is assumed.

---

## The `LIMIT` Clause

Restricts the number of rows returned:

```text
items limit 50
```

---

## Examples

```text
# Items whose path contains 'reports'
items where item_path:('reports')

# Changes involving validation failures
changes where val_new:(I) show default, val_old, val_new order by change_id desc
```

---

See also: [Interactive Mode](interactive_mode.md) · [Validators](validators.md) · [Configuration](configuration.md)


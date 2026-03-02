# time

Date, time, and timezone operations. Time objects are plain Forge objects with structured fields. Built on the `chrono` and `chrono-tz` crates.

## Time Object Structure

Most `time` functions return a time object with these fields:

| Field           | Type     | Description                        |
| --------------- | -------- | ---------------------------------- |
| `iso`           | `string` | ISO 8601 / RFC 3339 timestamp      |
| `unix`          | `int`    | Unix timestamp in seconds          |
| `unix_ms`       | `int`    | Unix timestamp in milliseconds     |
| `year`          | `int`    | Year                               |
| `month`         | `int`    | Month (1-12)                       |
| `day`           | `int`    | Day of month (1-31)                |
| `hour`          | `int`    | Hour (0-23)                        |
| `minute`        | `int`    | Minute (0-59)                      |
| `second`        | `int`    | Second (0-59)                      |
| `weekday`       | `string` | Full weekday name (e.g., "Monday") |
| `weekday_short` | `string` | Abbreviated weekday (e.g., "Mon")  |
| `day_of_year`   | `int`    | Day of the year (1-366)            |
| `timezone`      | `string` | Timezone name                      |

## Functions

### time.now(timezone?) -> object

Returns the current time as a time object. Defaults to UTC. Pass a timezone string for a specific zone.

```forge
let now = time.now()
say now.iso   // "2026-03-02T14:30:00+00:00"
say now.year  // 2026

let tokyo = time.now("Asia/Tokyo")
say tokyo.hour
```

### time.local() -> object

Returns the current time in the system's local timezone.

```forge
let local = time.local()
say local.timezone  // "Local"
```

### time.unix() -> int

Returns the current Unix timestamp in seconds.

```forge
let ts = time.unix()
say ts  // e.g. 1772618400
```

### time.today() -> string

Returns today's date as a `"YYYY-MM-DD"` string.

```forge
say time.today()  // "2026-03-02"
```

### time.date(year, month, day) -> object

Creates a time object for a specific date at midnight UTC.

```forge
let christmas = time.date(2026, 12, 25)
say christmas.weekday  // "Friday"
```

### time.parse(input, timezone?) -> object

Parses a date/time string or Unix timestamp into a time object. Supports multiple formats:

- `"2026-03-02T14:30:00Z"` (ISO 8601 with timezone)
- `"2026-03-02T14:30:00"` (ISO 8601 without timezone)
- `"2026-03-02 14:30:00"` (date + time)
- `"2026-03-02"` (date only)
- `"03/02/2026"` (US format MM/DD/YYYY)
- `"02.03.2026"` (European format DD.MM.YYYY)
- `"Mar 02, 2026"` (month name)
- `1772618400` (Unix timestamp as integer)

```forge
let t = time.parse("2026-03-02")
say t.weekday  // "Monday"

let t2 = time.parse(1772618400)
say t2.iso
```

### time.format(time_obj, format_str?) -> string

Formats a time object using a strftime-style format string. Defaults to `"%Y-%m-%d %H:%M:%S"`.

```forge
let now = time.now()
say time.format(now)                    // "2026-03-02 14:30:00"
say time.format(now, "%B %d, %Y")      // "March 02, 2026"
say time.format(now, "%H:%M")          // "14:30"
```

### time.from_unix(timestamp) -> object

Converts a Unix timestamp (seconds) to a time object.

```forge
let t = time.from_unix(0)
say t.iso  // "1970-01-01T00:00:00+00:00"
```

### time.diff(t1, t2) -> object

Returns the difference between two time objects.

| Field     | Type     | Description                                  |
| --------- | -------- | -------------------------------------------- |
| `seconds` | `int`    | Difference in seconds (negative if t1 < t2)  |
| `minutes` | `float`  | Difference in minutes                        |
| `hours`   | `float`  | Difference in hours                          |
| `days`    | `float`  | Difference in days                           |
| `weeks`   | `float`  | Difference in weeks                          |
| `human`   | `string` | Human-readable string (e.g., "2d 3h 15m 0s") |

```forge
let a = time.parse("2026-03-01")
let b = time.parse("2026-03-15")
let d = time.diff(b, a)
say d.days   // 14.0
say d.human  // "14d 0h 0m 0s"
```

### time.add(time_obj, duration) -> object

Adds a duration to a time object. Duration can be an object with time unit fields or an integer (seconds).

```forge
let now = time.now()
let later = time.add(now, { days: 7, hours: 3 })
let also_later = time.add(now, 3600)  // add 1 hour in seconds
```

Duration fields: `years`, `months`, `weeks`, `days`, `hours`, `minutes`, `seconds`, `millis`.

### time.sub(time_obj, duration) -> object

Subtracts a duration from a time object. Same interface as `time.add`.

```forge
let now = time.now()
let yesterday = time.sub(now, { days: 1 })
```

### time.zone(time_obj, timezone) -> object

Converts a time object to a different timezone.

```forge
let utc = time.now()
let eastern = time.zone(utc, "America/New_York")
say eastern.hour
```

### time.zones(filter?) -> array

Returns an array of all available timezone names. Optionally filter by substring.

```forge
let all = time.zones()
let us = time.zones("America")
```

### time.is_before(t1, t2) -> bool

Returns `true` if `t1` is before `t2`.

### time.is_after(t1, t2) -> bool

Returns `true` if `t1` is after `t2`.

### time.start_of(time_obj, unit) -> object

Returns the start of the given unit: `"day"`, `"hour"`, `"minute"`, `"week"`, `"month"`, `"year"`.

```forge
let now = time.now()
let start_of_day = time.start_of(now, "day")
say start_of_day.hour  // 0
```

### time.end_of(time_obj, unit) -> object

Returns the end of the given unit (23:59:59 for days, etc.).

### time.sleep(seconds) -> null

Pauses execution for the given number of seconds. Accepts integers or floats.

```forge
time.sleep(2)     // sleep 2 seconds
time.sleep(0.5)   // sleep 500ms
```

### time.elapsed() -> int

Returns the current time in milliseconds since the Unix epoch. Useful for measuring performance.

```forge
let start = time.elapsed()
// ... do work ...
let duration = time.elapsed() - start
say "Took " + str(duration) + "ms"
```

### time.is_weekend(time_obj?) -> bool

Returns `true` if the time falls on Saturday or Sunday. Defaults to now.

### time.is_weekday(time_obj?) -> bool

Returns `true` if the time falls on Monday-Friday. Defaults to now.

### time.day_of_week(time_obj?) -> string

Returns the full weekday name. Defaults to now.

### time.days_in_month(year?, month?) -> int

Returns the number of days in a month. Accepts `(year, month)` integers or a time object.

```forge
time.days_in_month(2024, 2)  // 29 (leap year)
time.days_in_month(2025, 2)  // 28
```

### time.is_leap_year(year?) -> bool

Returns `true` if the given year is a leap year. Accepts an integer or time object.

```forge
time.is_leap_year(2024)  // true
time.is_leap_year(2025)  // false
```

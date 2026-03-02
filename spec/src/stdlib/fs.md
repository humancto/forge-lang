# fs

File system operations. All paths are strings. Functions that write to the file system return `null` on success.

## Functions

### fs.read(path) -> string

Reads the entire file at `path` and returns its contents as a string.

```forge
let content = fs.read("config.txt")
say content
```

### fs.write(path, content) -> null

Writes `content` to the file at `path`, creating or overwriting the file.

```forge
fs.write("output.txt", "Hello, world!")
```

### fs.append(path, content) -> null

Appends `content` to the file at `path`. Creates the file if it does not exist.

```forge
fs.append("log.txt", "New log entry\n")
```

### fs.exists(path) -> bool

Returns `true` if a file or directory exists at `path`.

```forge
if fs.exists("config.json") {
    say "Config found"
}
```

### fs.list(path) -> array

Returns an array of file and directory names in the directory at `path`. Names only, not full paths.

```forge
let files = fs.list("./src")
// ["main.fg", "utils.fg", "lib"]
```

### fs.remove(path) -> null

Deletes a file or directory (recursively) at `path`.

```forge
fs.remove("temp.txt")
fs.remove("build/")     // removes directory and all contents
```

### fs.mkdir(path) -> null

Creates the directory at `path`, including any necessary parent directories.

```forge
fs.mkdir("build/output/logs")
```

### fs.copy(source, destination) -> int

Copies a file from `source` to `destination`. Returns the number of bytes copied.

```forge
let bytes = fs.copy("original.txt", "backup.txt")
say bytes  // e.g. 1024
```

### fs.rename(old_path, new_path) -> null

Renames or moves a file or directory.

```forge
fs.rename("draft.txt", "final.txt")
```

### fs.size(path) -> int

Returns the size of the file at `path` in bytes.

```forge
let s = fs.size("data.bin")
say s  // e.g. 4096
```

### fs.ext(path) -> string

Returns the file extension without the leading dot. Returns an empty string if none.

```forge
fs.ext("photo.png")     // "png"
fs.ext("Makefile")      // ""
```

### fs.read_json(path) -> any

Reads a JSON file and returns the parsed Forge value (object, array, etc.).

```forge
let config = fs.read_json("config.json")
say config.name
```

### fs.write_json(path, value) -> null

Serializes `value` as pretty-printed JSON and writes it to `path`.

```forge
let data = { name: "forge", version: "0.3.3" }
fs.write_json("package.json", data)
```

### fs.lines(path) -> array

Reads a file and returns an array of strings, one per line.

```forge
let lines = fs.lines("data.csv")
say len(lines)  // number of lines
```

### fs.dirname(path) -> string

Returns the directory portion of `path`.

```forge
fs.dirname("/home/user/file.txt")  // "/home/user"
```

### fs.basename(path) -> string

Returns the file name portion of `path`.

```forge
fs.basename("/home/user/file.txt")  // "file.txt"
```

### fs.join_path(a, b) -> string

Joins two path segments with the platform path separator.

```forge
fs.join_path("/home", "user")  // "/home/user"
```

### fs.is_dir(path) -> bool

Returns `true` if `path` is a directory.

```forge
fs.is_dir("/tmp")      // true
fs.is_dir("file.txt")  // false
```

### fs.is_file(path) -> bool

Returns `true` if `path` is a regular file.

```forge
fs.is_file("main.fg")  // true
fs.is_file("/tmp")      // false
```

### fs.temp_dir() -> string

Returns the path to the system temporary directory.

```forge
let tmp = fs.temp_dir()
say tmp  // e.g. "/tmp"
```

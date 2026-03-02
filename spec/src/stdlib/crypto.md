# crypto

Hashing and encoding utilities. All functions accept and return strings.

## Functions

### crypto.sha256(input) -> string

Returns the SHA-256 hash of `input` as a lowercase hex string.

```forge
crypto.sha256("hello")
// "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
```

### crypto.md5(input) -> string

Returns the MD5 hash of `input` as a lowercase hex string.

```forge
crypto.md5("hello")
// "5d41402abc4b2a76b9719d911017c592"
```

> **Note:** MD5 is cryptographically broken. Use `crypto.sha256` for security-sensitive hashing. MD5 is provided for compatibility and checksums only.

### crypto.base64_encode(input) -> string

Encodes `input` as a Base64 string using the standard alphabet.

```forge
crypto.base64_encode("hello world")
// "aGVsbG8gd29ybGQ="
```

### crypto.base64_decode(input) -> string

Decodes a Base64 string back to its original form.

```forge
crypto.base64_decode("aGVsbG8gd29ybGQ=")
// "hello world"
```

### crypto.hex_encode(input) -> string

Encodes `input` as a hexadecimal string.

```forge
crypto.hex_encode("AB")
// "4142"
```

### crypto.hex_decode(input) -> string

Decodes a hexadecimal string back to its original form.

```forge
crypto.hex_decode("4142")
// "AB"
```

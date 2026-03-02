# jwt

JSON Web Token (JWT) creation, verification, and decoding. Uses HMAC-SHA and RSA algorithms via the `jsonwebtoken` crate. Stateless module — no connections or state.

## Functions

### jwt.sign(claims, secret, options?) -> string

Creates a signed JWT from a claims object. Returns a dot-separated token string.

```forge
let token = jwt.sign({ user_id: 123, role: "admin" }, "my-secret")
```

With options:

```forge
let token = jwt.sign({ user_id: 123 }, "secret", {
    expires: "1h",
    issuer: "myapp",
    audience: "users",
    subject: "user-123",
    algorithm: "HS256"
})
```

**Options:**

| Key          | Type   | Default | Description                                 |
| ------------ | ------ | ------- | ------------------------------------------- |
| `expires`    | string | none    | Duration: `"1h"`, `"30m"`, `"7d"`, `"365d"` |
| `issuer`     | string | none    | Sets the `iss` claim                        |
| `audience`   | string | none    | Sets the `aud` claim                        |
| `subject`    | string | none    | Sets the `sub` claim                        |
| `algorithm`  | string | `HS256` | `HS256`, `HS384`, `HS512`, `RS256`, `ES256` |
| `not_before` | string | none    | Duration before token becomes valid         |

An `iat` (issued at) claim is automatically added if not already present in the claims object.

### jwt.verify(token, secret) -> object

Verifies the token signature and expiration, then returns the decoded claims as an object. Returns an error if the token is expired, has an invalid signature, or is malformed.

```forge
let claims = jwt.verify(token, "my-secret")
say claims.user_id  // 123
say claims.role     // "admin"
```

### jwt.decode(token) -> object

Decodes the token **without** verifying the signature. Useful for debugging or inspecting tokens in middleware. Returns an object with `header` and `payload` fields.

```forge
let decoded = jwt.decode(token)
say decoded.header.alg      // "HS256"
say decoded.header.typ      // "JWT"
say decoded.payload.user_id // 123
```

### jwt.valid(token, secret) -> bool

Convenience function that returns `true` if the token is valid, `false` otherwise. Never throws an error.

```forge
if jwt.valid(token, "secret") {
    say "Token is valid"
} else {
    say "Token is invalid or expired"
}
```

## Example

```forge
let secret = "my-super-secret-key-2024"

// Create a token with claims and options
let token = jwt.sign({
    user_id: 123,
    name: "Alice",
    role: "admin"
}, secret, {
    expires: "1h",
    issuer: "forge-app"
})

say "Token: " + token

// Verify and extract claims
let claims = jwt.verify(token, secret)
say "User: " + claims.name    // Alice
say "Role: " + claims.role    // admin

// Decode without verification (for debugging)
let decoded = jwt.decode(token)
say "Algorithm: " + decoded.header.alg  // HS256

// Quick validity check
say jwt.valid(token, secret)         // true
say jwt.valid(token, "wrong-key")    // false
```

## Supported Algorithms

| Algorithm | Type  | Key Format    |
| --------- | ----- | ------------- |
| HS256     | HMAC  | Secret string |
| HS384     | HMAC  | Secret string |
| HS512     | HMAC  | Secret string |
| RS256     | RSA   | PEM string    |
| RS384     | RSA   | PEM string    |
| RS512     | RSA   | PEM string    |
| ES256     | ECDSA | PEM string    |
| ES384     | ECDSA | PEM string    |

The `none` algorithm is explicitly rejected for security reasons.

## Notes

- Duration strings support: `s` (seconds), `m` (minutes), `h` (hours), `d` (days), `w` (weeks).
- `jwt.verify` validates the `exp` claim with zero leeway — expired tokens are immediately rejected.
- For RSA and ECDSA algorithms, the secret parameter should be a PEM-encoded key string.
- The `jwt` module is available in both interpreter and VM execution modes.

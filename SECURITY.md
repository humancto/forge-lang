# Security Policy

## Supported Versions

| Version | Supported |
| ------- | --------- |
| 0.3.x   | Yes       |
| 0.2.x   | No        |
| < 0.2   | No        |

## Reporting a Vulnerability

If you discover a security vulnerability in Forge, **please do not open a public issue.**

Instead, report it privately:

1. Go to [Security Advisories](https://github.com/humancto/forge-lang/security/advisories/new)
2. Or email: **security@forge-lang.dev**

Include:

- Description of the vulnerability
- Steps to reproduce
- Potential impact
- Suggested fix (if any)

We will acknowledge your report within 48 hours and provide a timeline for a fix.

## Known Security Considerations

Forge v0.3.0 is a young language. The following are documented limitations, not vulnerabilities:

- **SQL queries** use raw strings — no parameterized query API yet. Do not pass untrusted user input directly into `db.query()` or `pg.query()`.
- **File system** access is unrestricted — `fs.read/write` can access any path the process has permission for.
- **Shell execution** via `sh()` and `exec.run_command()` runs commands directly. Do not pass untrusted input.
- **HTTP server** uses permissive CORS by default. Configure appropriately for production.

## Security Best Practices

When using Forge:

- Sanitize all user input before passing to `db.query()`, `sh()`, or `fs.*` functions
- Use environment variables (`env.get()`) for secrets, never hardcode them
- Bind servers to `127.0.0.1` for local development (this is the default)

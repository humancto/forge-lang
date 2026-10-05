# RFC 0007: Package Registry — a Git-Hosted Sparse Index

- **Status:** Implemented (client and publishing tool); hosted index pending
- **Author:** Forge maintainers
- **Date:** 2026-10-05

## Summary

Forge packages are published to a **sparse index**: a Git repository of
static files, served over HTTPS (by default from
`raw.githubusercontent.com/humancto/forge-registry/main`) and readable from
any mirror or `file://` path. Each package has one file listing its versions
as JSON lines. Each line carries the version's dependencies, the mandatory
SHA-256 of its `.tar.gz` archive, the archive's URL (normally a GitHub
release asset), a `yanked` flag and an optional ed25519 publisher
signature. Clients fetch only the files they need, cache them with ETag
revalidation and work offline from the cache. Publishing is a pull request
against the index repository. The CLI produces the archive and the
commit, CI re-validates the entry, and no API server or login is involved.

## Motivation

Before this RFC, `forge add router` fell back to
`raw.githubusercontent.com/forge-lang/registry/main/packages/router.toml`,
a repository that does not exist. Every remote lookup returned 404, and the
404 looked the same as "package not found". The local registry
(`~/.forge/registry`, `forge publish`) worked, but nothing could be shared.

Running a registry *service* (database, upload API, accounts, storage) is
out of proportion for a young language, and it is one more thing to secure.
Package ecosystems at every scale have shown that the hard parts of a
registry are the **index format**, **integrity** and **trust**, not the
hosting:

- crates.io moved from a Git index clone to a *sparse* HTTP index: one
  small file per crate, fetched on demand and cached with ETags.
- Go's module proxy plus checksum database shows that content addressing
  (checksums pinned in a lockfile) makes mirrors safe by construction.
- Homebrew and Nix run large ecosystems as reviewed pull requests against a
  Git repository.

This design combines those ideas with GitHub as the only infrastructure.

## Design

### Repository layout

```text
config.json              {"v": 1}  (+ optional "dl" mirror template)
index.toml               search summary: name, description, latest version
owners.toml              optional: namespace and package key ownership
index/1/a                packages with 1-character names
index/2/ab               2-character names
index/3/a/abc            3-character names: first char
index/ro/ut/router       4+ characters: first two / next two
.github/workflows/       CI that validates every PR (tools/registry-template/)
CODEOWNERS
```

Sharding follows crates.io, so no directory grows without bound and a
package's path is computable from its name alone (`index::index_path`).

### `config.json`

```json
{ "v": 1, "dl": "https://mirror.example/{name}/{vers}/{cksum}.tar.gz" }
```

`v` is the index format version. A client refuses an index whose `v` is
newer than it understands and tells the user to upgrade. `dl` is
optional. When set (typically by a mirror), it replaces each entry's own
`url`. `{name}`, `{vers}` and `{cksum}` are substituted. A missing
`config.json` means "this URL is not a Forge registry". The client says
exactly that, and for the default URL it explains that the hosted index has
not been created yet, instead of reporting every package as missing.

### Package files

One JSON object per line, one line per published version, in publish
order. Field order is fixed (it is the serialization order) so diffs are
one added line:

```json
{"v":1,"name":"router","vers":"1.2.0","deps":[{"name":"json-utils","req":"^0.3"}],"cksum":"9f2c…64 hex…","url":"https://github.com/acme/router/releases/download/v1.2.0/router-1.2.0.tar.gz","yanked":false,"pubkey":"ed25519:3q2+…","sig":"Vb1…","description":"HTTP router","license":"MIT","published":"2026-10-05T12:00:00Z"}
```

| Field | Required | Meaning |
|---|---|---|
| `v` | yes | Entry format version. Lines with a newer `v` are skipped, so old clients survive format growth. |
| `name` | yes | Must equal the file name. |
| `vers` | yes | Semantic version. Unique within the file. |
| `deps` | no | Registry dependencies: `name` + semver `req`. Packages with `git`/`path` dependencies cannot be published. |
| `cksum` | **yes** | Lowercase hex SHA-256 of the archive. Entries without one are invalid, and the client will not install them. |
| `url` | yes | Archive location (`https://`; `http://`/`file://` are for tests and private mirrors). |
| `yanked` | no | See *Yanking*. |
| `pubkey`, `sig` | together | Optional signature (see *Security*). |
| `description`, `license`, `published` | no | Informational. |

Malformed lines are an **error**, not skipped, because silently dropping a
version could change resolution. Lines with a newer `v` are the only
exception.

### Archives

`forge publish` builds a deterministic `<name>-<version>.tar.gz`:

- sorted paths under a `<name>-<version>/` prefix;
- zero mtimes, uid and gid; mode 0644;
- a gzip header with no timestamp.

The same sources always produce the same checksum. Contents are the files
`forge publish` already selects (`*.fg`, `forge.toml`, README) minus the
default excludes. On install, the client extracts only regular files and
directories. It rejects absolute paths, `..` components, symlinks, hard
links and device nodes before anything is moved into `forge_modules/`.

### Names and namespacing

- 1–64 characters: lowercase ASCII letters, digits, `-` and `_`. A name
  starts with a letter, does not end with `-` or `_`, and has no `--` or `__`.
- Reserved: the stdlib globals (`json`, `http`, `fs`, ...), toolchain words
  (`forge`, `std`, `core`, `test`, `forge_modules`, ...) and Windows device
  names (`con`, `nul`, `com1`, ...).
- **Look-alikes:** `foo_bar` and `foo-bar` are the same name for
  registration (`index::canonical_name`). The second one is rejected.
- **Namespaces** are prefix ownership, declared in `owners.toml`:

  ```toml
  [namespaces.acme]          # owns every package named acme-*
  keys = ["ed25519:…"]
  github = ["@acme/maintainers"]

  [packages.router]          # owns one package; also how keys rotate
  keys = ["ed25519:old…", "ed25519:new…"]
  ```

  A name covered by a rule must be signed by one of its keys. Names stay
  flat (an import is `import "router"`, not `@acme/router`), so namespacing
  needs no change to import resolution. Scoped names can be added later as
  a new entry format version if prefix ownership proves insufficient.

### Resolution and the lockfile

`forge add` / `forge install` resolve a `VersionReq` against the entries.
The newest **non-yanked** match wins. A version already recorded in
`forge.lock` is kept as long as it still satisfies the requirement, even if
it was yanked later. `forge update` ignores locked versions and re-resolves.
Local registries (`FORGE_REGISTRY_PATH`, `./.forge/registry`,
`~/.forge/registry`) are consulted first, as before.

A sparse-registry lock entry pins content, not only a version:

```toml
[[packages]]
name = "router"
version = "1.2.0"
source = "sparse+https://raw.githubusercontent.com/humancto/forge-registry/main"
checksum = "…"                      # directory-sha256 of the installed tree (unchanged)
checksum_kind = "directory-sha256"
archive_checksum = "sha256:9f2c…"   # the index cksum at install time
signer = "ed25519:3q2+…"            # present when the version was signed
```

Reinstalling a locked version whose index `cksum` or signer has changed is a
hard error. A published version's content must never change. If it does,
the index was tampered with or rewritten.

### Yanking

`forge yank router@1.2.0 --registry <index-clone>` flips `yanked` to `true`
on a `yank/router-1.2.0` branch and refreshes the `index.toml` row. `--undo`
reverses it. Yanked versions are never chosen by new resolutions but stay
installable from a lockfile. Nothing is ever deleted: deleting a version
would break every lockfile that names it. Malware is the exception. It is
handled by maintainers removing the archive and, if needed, the line, and
the change is recorded in the index's Git history. The signature does not
cover `yanked`. Whether a version is yanked is an index policy decision,
made by reviewed PRs.

### Security

1. **Checksums are mandatory.** Every entry carries the archive's SHA-256.
   The client verifies it before extraction. Archives are cached
   content-addressed (`~/.forge/cache/archives/<name>-<vers>-<cksum16>.tar.gz`)
   and re-verified on every use.
2. **Optional publisher signatures (ed25519).** `forge publish --sign`
   signs `"forge-registry-v1\n<name>\n<vers>\n<cksum>\n"` with the key in
   `~/.forge/keys/publish.key` (or `$FORGE_SIGNING_KEY`), creating it with
   mode 0600 on first use. Binding name and version prevents replaying a
   signature onto another package or version. Binding the checksum binds
   the content. Keys are written as `ed25519:<base64>`. We use raw ed25519
   (`ed25519-dalek`, RustCrypto/dalek) rather than the minisign file format:
   the signed payload is a short string, and keys live inside the index
   entry rather than in sidecar files. Minisign's trusted comments and key
   IDs would add format surface without adding security here.
3. **Trust on first use, per publisher.** The client pins the first key it
   sees for each (registry URL, package) pair in
   `~/.forge/trusted-keys.toml`. Afterwards:
   - a version signed by a different key is refused (with instructions to
     remove the pin after verifying a rotation out of band);
   - an unsigned version of a package that was signed before is refused
     (signature stripping);
   - `FORGE_REQUIRE_SIGNATURES=1` refuses unsigned packages entirely.
   The lockfile's `signer` extends the same check to CI machines that
   have never seen the package.
4. **Index-side continuity.** The index CI (and `forge publish`) refuse a
   new version signed by a key other than the one on the newest signed
   version, unless `owners.toml` lists the new key for that package or its
   namespace. A compromised publisher key can therefore not be swapped
   silently. Rotation is a reviewed change to `owners.toml`.
5. **Credentials.** `GITHUB_TOKEN` is sent only over HTTPS to GitHub hosts
   (`raw.githubusercontent.com`, `github.com`, ...). It is never sent to
   `FORGE_REGISTRY_URL` mirrors or archive URLs on other hosts.
6. **Bounds.** Index files are capped at 16 MiB and archives at 64 MiB.

**Threat model.** A compromised GitHub account that controls the index
repository can add new packages and versions. TOFU pins and lockfile
signers protect users who already depend on a package. Mandatory review
and CI on the index repository protect new installs. A compromised mirror
or CDN cannot change content without breaking checksums, signatures or
lockfile pins. A transparency log (as in Go's sumdb) is future work.

### Cache, offline and mirrors

- Index files are cached under
  `~/.forge/cache/registry/<host>-<hash(url)>/`, one directory per
  registry URL. The cache stays fresh for `FORGE_CACHE_TTL` seconds
  (default 300). After that the client revalidates with `If-None-Match`,
  and a `304` refreshes the timestamp.
- On a network failure or a 5xx, a cached copy is used with a warning.
- `FORGE_OFFLINE=1` never touches the network. Uncached files are a clear
  error.
- `FORGE_REGISTRY_URL` selects the registry or a mirror. A mirror is any
  static copy of the index repository. Setting `dl` in its `config.json`
  also redirects archive downloads. Checksums make mirrors untrusted by
  design.
- `file:///path/to/index-clone` works for air-gapped setups and tests, and
  is never cached.

### Publishing flow

```bash
git clone https://github.com/humancto/forge-registry ../forge-registry
forge publish --sign --registry ../forge-registry
#   ✓ Added router v1.2.0 to index ../forge-registry
#     Archive: dist/router-1.2.0.tar.gz
#   Next steps:
#     1. Upload the archive so it is served at https://github.com/acme/router/releases/download/v1.2.0/router-1.2.0.tar.gz
#     2. Push branch 'publish/router-1.2.0' and open a pull request against the index
gh release create v1.2.0 dist/router-1.2.0.tar.gz
git -C ../forge-registry push -u origin publish/router-1.2.0 && gh pr create ...
```

`--registry` switches to index mode when the directory has a
`config.json`. Otherwise it is the existing local-registry publish.
`--download-url` overrides the archive URL template. The default is a
GitHub release asset of `project.repository`. `--out-dir` chooses where
the archive is written, and `--no-commit` skips the branch and commit.
The index clone must be clean, so the commit contains only this entry.

### Hosting the default index

The repository `github.com/humancto/forge-registry` must be created by the
maintainer and seeded from `tools/registry-template/` (README, `config.json`,
empty `index.toml`, `owners.toml`, CODEOWNERS, validation CI). Until it
exists, remote lookups fail with:

```
Error: package 'router' is not in a local registry, and the remote registry is unavailable:
  https://raw.githubusercontent.com/humancto/forge-registry/main is not a Forge registry (no config.json found).
  The default hosted registry (github.com/humancto/forge-registry) has not been published yet.
  Use a local registry (`forge publish`, FORGE_REGISTRY_PATH) or point FORGE_REGISTRY_URL at a mirror.
```

## Alternatives Considered

- **A registry service with an upload API (crates.io / npm model).** This
  is the best long-term user experience, but it needs accounts, tokens,
  storage and operations from day one. The sparse format here is what such
  a service would serve anyway, so a service can be added later behind the
  same client.
- **Full Git index clone (old crates.io).** Clone time grows with the
  ecosystem, and the client would need a Git implementation. Sparse HTTP
  needs neither.
- **One TOML file per package (the previous, never-deployed design).**
  Each update rewrites a whole file, which makes concurrent PRs conflict. JSON
  lines are append-only and diff as one line.
- **Git tags as the only source (Go modules without a proxy).** No central
  place for yanks, ownership, search or checksums, and every install needs
  Git.
- **Mandatory signatures.** This raises the barrier to a first publish.
  TOFU plus lockfile pins catch the attacks that matter for existing users,
  and `FORGE_REQUIRE_SIGNATURES` lets organisations opt in. Mandatory
  signing can be switched on per namespace through `owners.toml` today.
- **minisign / Sigstore.** Minisign: see *Security*. Sigstore keyless
  signing ties identity to OIDC providers and needs online transparency
  infrastructure. It remains a good future addition (`sig` could gain a
  `sigstore` variant under a new `v`).

## Implementation Notes

| Piece | Where |
|---|---|
| Index format, name rules, resolution, ownership | `src/registry/index.rs` (pure functions) |
| Sparse client: cache, ETag, offline, mirrors, archives | `src/registry/client.rs` |
| ed25519 signing, TOFU trust store | `src/registry/signing.rs` |
| Safe extraction, search | `src/registry/mod.rs` |
| Install, lock pins | `src/package.rs` (`install_from_remote_registry`) |
| `forge publish --registry <index>`, `forge yank` | `src/publish_index.rs` |
| Index repository seed and CI validator | `tools/registry-template/` |
| End-to-end tests (file:// index, local HTTP server with ETag) | `tests/registry_index.rs` |

The index CI validator (`tools/registry-template/scripts/validate_index.py`)
re-implements the format rules independently in Python. The Rust test
suite runs it against an index produced by `forge publish`, so the two
implementations cannot drift apart unnoticed.

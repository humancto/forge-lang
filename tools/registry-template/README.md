# Forge package registry (index)

This repository is the **sparse index** of the Forge package registry. The
format and the trust model are specified in
[forge-lang RFC 0007](https://github.com/humancto/forge-lang/blob/main/rfcs/0007-package-registry.md).
Forge reads it from
`https://raw.githubusercontent.com/humancto/forge-registry/main` (override
with `FORGE_REGISTRY_URL`).

Package sources are not stored here. Each version is a single JSON line
pointing at a `.tar.gz` archive (normally a GitHub release asset of the
package's own repository), with its mandatory SHA-256 and an optional
ed25519 signature.

## Layout

| Path | What |
|---|---|
| `config.json` | Index format version (`v`) and optional mirror `dl` template |
| `index.toml` | Search summary used by `forge search` (generated) |
| `owners.toml` | Which signing keys own which names / namespaces |
| `index/…` | One file per package, one JSON line per version |
| `scripts/validate_index.py` | The CI validator (stdlib + `cryptography`) |

## Publishing a package

```bash
git clone https://github.com/humancto/forge-registry
cd my-package
forge publish --sign --registry ../forge-registry
# Upload the archive to the URL printed (by default a release asset of
# project.repository):
gh release create v1.2.0 dist/my-package-1.2.0.tar.gz
# Open the pull request:
git -C ../forge-registry push -u origin publish/my-package-1.2.0
gh pr create --repo humancto/forge-registry --fill
```

`forge publish` validates the manifest, builds a deterministic archive,
signs the entry (with `--sign`) and commits `index/…` + `index.toml` on a
fresh `publish/<name>-<version>` branch. CI then re-checks everything,
including downloading your archive and comparing its checksum. Upload the
archive **before** opening the PR.

Rules enforced by CI:

- Names: lowercase `[a-z0-9_-]`, starting with a letter, 1–64 chars.
  Names cannot be reserved (stdlib modules like `json`, `http`) or look
  like an existing name (`foo_bar` vs `foo-bar`).
- Versions are semver and **immutable**: a published line never changes,
  except its `yanked` flag. Lines and files are never removed.
- Checksums are mandatory and must match the archive.
- Once a package has a signed version, later versions must be signed by
  the same key. `owners.toml` can require signatures for a name or a
  `prefix-*` namespace, and is how keys are rotated.
- A package PR may only touch `index/` and `index.toml`.
- Dependencies must name packages that exist in this index. Packages
  with git/path dependencies cannot be published.

## Yanking

```bash
forge yank my-package@1.2.0 --registry ../forge-registry          # or --undo
git -C ../forge-registry push -u origin yank/my-package-1.2.0
```

A yanked version is no longer chosen by new resolutions. Projects whose
`forge.lock` already pins it keep working.

## Mirrors

Any static copy of this repository is a valid mirror
(`FORGE_REGISTRY_URL=https://mirror.example/forge-registry`). To also mirror
archives, set `"dl": "https://mirror.example/archives/{name}/{vers}/{cksum}.tar.gz"`
in the mirror's `config.json`. Checksums and signatures make mirrors
untrusted by design.

## Maintainers

1. Create `github.com/humancto/forge-registry` from the contents of
   `tools/registry-template/` in forge-lang.
2. Replace the placeholder owners in `.github/CODEOWNERS`.
3. Protect `main`: require pull requests, the **Validate index** check,
   and Code Owner review. Disallow force pushes.
4. Run the validator locally with
   `python3 scripts/validate_index.py --root . --base origin/main`.

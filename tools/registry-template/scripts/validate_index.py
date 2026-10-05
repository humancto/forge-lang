#!/usr/bin/env python3
"""Validate a Forge sparse package index (rfcs/0007-package-registry.md).

This is an independent re-implementation of the rules in
forge-lang/src/registry/index.rs. The Forge test suite runs it against an
index written by `forge publish`, so the two cannot drift apart unnoticed.

Checks:
  * config.json, index layout (file path derived from the name), name rules,
    reserved and look-alike names
  * every entry: format, semver version, dependency requirements, mandatory
    lowercase SHA-256 checksum, URL scheme, pubkey/sig pairing
  * dependencies name packages that exist in the index
  * ed25519 signatures (requires the `cryptography` package)
  * owners.toml: namespace/package key ownership; key continuity once signed
  * index.toml matches the entries (latest non-yanked version, description)
  * with --base REF: append-only history (published lines never change,
    except the `yanked` flag; nothing is removed)
  * archive checksums: downloads each new/changed entry's archive (all
    entries without --base) and compares SHA-256; --offline skips this

Exit status 0 when valid, 1 with one line per problem otherwise.
Standard library only, plus `cryptography` for signatures.
"""

import argparse
import base64
import hashlib
import json
import os
import re
import subprocess
import sys
import urllib.request

try:  # Python 3.11+
    import tomllib
except ModuleNotFoundError:  # pragma: no cover
    tomllib = None

FORMAT_VERSION = 1
MAX_NAME_LEN = 64
MAX_ARCHIVE_BYTES = 64 * 1024 * 1024
RESERVED = {
    "forge", "std", "core", "stdlib", "test", "tests", "forge_modules", "forge-modules",
    "math", "fs", "io", "crypto", "db", "pg", "mysql", "jwt", "env", "json", "regex",
    "log", "http", "csv", "term", "os", "path", "time", "url", "toml", "npc", "ws", "exec",
    "con", "prn", "aux", "nul",
    *[f"com{i}" for i in range(1, 10)],
    *[f"lpt{i}" for i in range(1, 10)],
}
NAME_RE = re.compile(r"^[a-z][a-z0-9_-]*$")
SHA256_RE = re.compile(r"^[0-9a-f]{64}$")
# https://semver.org/#is-there-a-suggested-regular-expression-regex-to-check-a-semver-string
SEMVER_RE = re.compile(
    r"^(0|[1-9]\d*)\.(0|[1-9]\d*)\.(0|[1-9]\d*)"
    r"(?:-((?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*)(?:\.(?:0|[1-9]\d*|\d*[a-zA-Z-][0-9a-zA-Z-]*))*))?"
    r"(?:\+([0-9a-zA-Z-]+(?:\.[0-9a-zA-Z-]+)*))?$"
)
# One comparator of a Cargo-style VersionReq (what Rust's `semver` accepts).
COMPARATOR_RE = re.compile(
    r"^(?:\^|~|=|>=|<=|>|<)?\s*"
    r"(?:\*|(?:0|[1-9]\d*)(?:\.(?:\*|x|X|0|[1-9]\d*)(?:\.(?:\*|x|X|0|[1-9]\d*))?)?)"
    r"(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$"
)
SIGNED_PREFIX = "forge-registry-v1"


def name_problem(name):
    if not name or len(name) > MAX_NAME_LEN:
        return f"must be 1 to {MAX_NAME_LEN} characters long"
    if not NAME_RE.match(name):
        return "must start with a lowercase letter and use only [a-z0-9_-]"
    if name.endswith(("-", "_")) or "--" in name or "__" in name:
        return "must not end with '-'/'_' or repeat them"
    if name in RESERVED:
        return "is reserved"
    return None


def canonical(name):
    return name.lower().replace("_", "-")


def index_path(name):
    n = name.lower()
    if len(n) == 1:
        return f"index/1/{n}"
    if len(n) == 2:
        return f"index/2/{n}"
    if len(n) == 3:
        return f"index/3/{n[0]}/{n}"
    return f"index/{n[:2]}/{n[2:4]}/{n}"


def semver_key(vers):
    m = SEMVER_RE.match(vers)
    major, minor, patch, pre = int(m.group(1)), int(m.group(2)), int(m.group(3)), m.group(4)
    if pre is None:
        return (major, minor, patch, 1, ())
    parts = tuple((0, int(p), "") if p.isdigit() else (1, 0, p) for p in pre.split("."))
    return (major, minor, patch, 0, parts)


def valid_req(req):
    req = req.strip()
    if req == "*":
        return True
    return all(COMPARATOR_RE.match(c.strip()) for c in req.split(",")) and req != ""


def signed_message(e):
    return f"{SIGNED_PREFIX}\n{e['name']}\n{e['vers']}\n{e['cksum']}\n".encode()


class Validator:
    def __init__(self, root, base=None, offline=False, require_signatures=False,
                 allow_insecure_urls=False):
        self.root = root
        self.base = base
        self.offline = offline
        self.require_signatures = require_signatures
        self.allow_insecure_urls = allow_insecure_urls
        self.problems = []
        self.packages = {}  # name -> [entries in file order]

    def err(self, where, msg):
        self.problems.append(f"{where}: {msg}")

    # ---- loading -------------------------------------------------------
    def load(self):
        cfg_path = os.path.join(self.root, "config.json")
        try:
            with open(cfg_path, encoding="utf-8") as f:
                cfg = json.load(f)
            if cfg.get("v") != FORMAT_VERSION:
                self.err("config.json", f"v must be {FORMAT_VERSION}")
            unknown = set(cfg) - {"v", "dl"}
            if unknown:
                self.err("config.json", f"unknown keys {sorted(unknown)}")
        except (OSError, ValueError) as e:
            self.err("config.json", f"missing or invalid: {e}")

        index_root = os.path.join(self.root, "index")
        for dirpath, _dirs, files in os.walk(index_root):
            for fname in sorted(files):
                if fname.startswith("."):  # .gitkeep and friends
                    continue
                full = os.path.join(dirpath, fname)
                rel = os.path.relpath(full, self.root).replace(os.sep, "/")
                problem = name_problem(fname)
                if problem:
                    self.err(rel, f"package name '{fname}' {problem}")
                    continue
                if rel != index_path(fname):
                    self.err(rel, f"must live at {index_path(fname)}")
                    continue
                self.packages[fname] = self.parse_file(rel, fname, full)

    def parse_file(self, rel, name, full):
        entries = []
        seen = set()
        with open(full, encoding="utf-8") as f:
            for lineno, line in enumerate(f, 1):
                line = line.strip()
                if not line:
                    continue
                where = f"{rel}:{lineno}"
                try:
                    e = json.loads(line)
                except ValueError as ex:
                    self.err(where, f"invalid JSON: {ex}")
                    continue
                if not isinstance(e, dict):
                    self.err(where, "entry must be a JSON object")
                    continue
                if isinstance(e.get("v"), int) and e["v"] > FORMAT_VERSION:
                    self.err(where, f"entry format v{e['v']} is newer than this validator")
                    continue
                self.check_entry(where, name, e)
                if e.get("vers") in seen:
                    self.err(where, f"version {e.get('vers')} listed twice")
                seen.add(e.get("vers"))
                e["_where"] = where
                e["_line"] = line
                entries.append(e)
        return entries

    # ---- per-entry rules -----------------------------------------------
    def check_entry(self, where, name, e):
        allowed = {"v", "name", "vers", "deps", "cksum", "url", "yanked", "pubkey", "sig",
                   "description", "license", "published"}
        for key in set(e) - allowed:
            self.err(where, f"unknown field '{key}'")
        if e.get("v") != FORMAT_VERSION:
            self.err(where, f"v must be {FORMAT_VERSION}")
        if e.get("name") != name:
            self.err(where, f"name must be '{name}' (the file name)")
        vers = e.get("vers")
        if not isinstance(vers, str) or not SEMVER_RE.match(vers):
            self.err(where, f"vers {vers!r} is not valid semver")
        if not isinstance(e.get("cksum"), str) or not SHA256_RE.match(e["cksum"]):
            self.err(where, "cksum must be a lowercase hex SHA-256 (checksums are mandatory)")
        url = e.get("url")
        schemes = ("https://", "http://", "file://") if self.allow_insecure_urls else ("https://",)
        if not isinstance(url, str) or not url.startswith(schemes):
            self.err(where, f"url must start with {' or '.join(schemes)}")
        if not isinstance(e.get("yanked", False), bool):
            self.err(where, "yanked must be a boolean")
        deps = e.get("deps", [])
        if not isinstance(deps, list):
            self.err(where, "deps must be a list")
            deps = []
        for d in deps:
            if not isinstance(d, dict) or set(d) != {"name", "req"}:
                self.err(where, f"dependency {d!r} must have exactly 'name' and 'req'")
                continue
            problem = name_problem(d["name"])
            if problem:
                self.err(where, f"dependency name '{d['name']}' {problem}")
            if not isinstance(d["req"], str) or not valid_req(d["req"]):
                self.err(where, f"dependency '{d['name']}' has invalid requirement {d['req']!r}")
        has_pk, has_sig = "pubkey" in e, "sig" in e
        if has_pk != has_sig:
            self.err(where, "pubkey and sig must be present together")
        elif has_pk:
            self.check_signature(where, e)
        elif self.require_signatures:
            self.err(where, "entry is unsigned and --require-signatures is set")

    def check_signature(self, where, e):
        try:
            from cryptography.exceptions import InvalidSignature
            from cryptography.hazmat.primitives.asymmetric.ed25519 import Ed25519PublicKey
        except ImportError:
            self.err(where, "cannot verify signature: pip install cryptography")
            return
        pk = e.get("pubkey", "")
        if not isinstance(pk, str) or not pk.startswith("ed25519:"):
            self.err(where, "pubkey must be 'ed25519:<base64>'")
            return
        try:
            raw = base64.b64decode(pk[len("ed25519:"):], validate=True)
            sig = base64.b64decode(e.get("sig", ""), validate=True)
            Ed25519PublicKey.from_public_bytes(raw).verify(sig, signed_message(e))
        except (ValueError, InvalidSignature, TypeError):
            self.err(where, f"signature verification FAILED for {pk}")

    # ---- cross-entry rules ---------------------------------------------
    def check_cross(self):
        names = sorted(self.packages)
        by_canon = {}
        for n in names:
            by_canon.setdefault(canonical(n), []).append(n)
        for group in by_canon.values():
            if len(group) > 1:
                self.err("index", f"look-alike package names: {', '.join(group)}")

        for name, entries in self.packages.items():
            for e in entries:
                for d in e.get("deps", []) or []:
                    if isinstance(d, dict) and d.get("name") not in self.packages:
                        self.err(e["_where"], f"dependency '{d.get('name')}' is not in the index")

        owners = self.load_owners()
        for name, entries in self.packages.items():
            required = self.required_keys(owners, name)
            last_signed = None  # (semver key, pubkey) of the newest signed so far
            for e in entries:
                pk = e.get("pubkey")
                if required is not None:
                    if pk is None:
                        self.err(e["_where"], f"'{name}' is owned in owners.toml and must be signed")
                    elif pk not in required:
                        self.err(e["_where"], f"{pk} is not an owner key of '{name}'")
                elif last_signed is not None and pk != last_signed[1]:
                    self.err(e["_where"],
                             f"must be signed by {last_signed[1]} like earlier versions "
                             f"(rotate keys via [packages.{name}] in owners.toml)")
                if pk is not None and isinstance(e.get("vers"), str) and SEMVER_RE.match(e["vers"]):
                    k = semver_key(e["vers"])
                    if last_signed is None or k > last_signed[0]:
                        last_signed = (k, pk)

    def load_owners(self):
        path = os.path.join(self.root, "owners.toml")
        if not os.path.exists(path):
            return {"namespaces": {}, "packages": {}}
        if tomllib is None:
            self.err("owners.toml", "Python 3.11+ (tomllib) is required to read owners.toml")
            return {"namespaces": {}, "packages": {}}
        try:
            with open(path, "rb") as f:
                data = tomllib.load(f)
        except (OSError, ValueError) as e:
            self.err("owners.toml", f"invalid: {e}")
            return {"namespaces": {}, "packages": {}}
        data.setdefault("namespaces", {})
        data.setdefault("packages", {})
        return data

    @staticmethod
    def required_keys(owners, name):
        if name in owners["packages"]:
            return owners["packages"][name].get("keys", [])
        prefix = name.split("-", 1)[0]
        if prefix != name and prefix in owners["namespaces"]:
            return owners["namespaces"][prefix].get("keys", [])
        return None

    def check_search_index(self):
        path = os.path.join(self.root, "index.toml")
        if tomllib is None:
            self.err("index.toml", "Python 3.11+ (tomllib) is required")
            return
        try:
            with open(path, "rb") as f:
                rows = tomllib.load(f).get("packages", [])
        except (OSError, ValueError) as e:
            self.err("index.toml", f"missing or invalid: {e}")
            return
        expected = {}
        for name, entries in self.packages.items():
            live = [e for e in entries if not e.get("yanked")
                    and isinstance(e.get("vers"), str) and SEMVER_RE.match(e["vers"])]
            if live:
                top = max(live, key=lambda e: semver_key(e["vers"]))
                expected[name] = (top["vers"], top.get("description", ""))
        actual = {r.get("name"): (r.get("latest", ""), r.get("description", "")) for r in rows}
        if actual != expected:
            missing = sorted(set(expected) - set(actual))
            extra = sorted(set(actual) - set(expected))
            wrong = sorted(n for n in set(actual) & set(expected) if actual[n] != expected[n])
            self.err("index.toml", f"out of date (missing {missing}, extra {extra}, "
                                   f"stale {wrong}); re-run forge publish/yank")

    # ---- history ---------------------------------------------------------
    def changed_entries(self):
        """Entries new or changed relative to --base (all entries without it)."""
        if not self.base:
            return [e for entries in self.packages.values() for e in entries]
        changed = []
        for name, entries in self.packages.items():
            rel = index_path(name)
            old = subprocess.run(["git", "-C", self.root, "show", f"{self.base}:{rel}"],
                                 capture_output=True, text=True)
            old_lines = [l.strip() for l in old.stdout.splitlines() if l.strip()] \
                if old.returncode == 0 else []
            if len(entries) < len(old_lines):
                self.err(rel, "published versions were removed (the index is append-only)")
            for i, e in enumerate(entries):
                if i >= len(old_lines):
                    changed.append(e)
                    continue
                before = json.loads(old_lines[i])
                now = {k: v for k, v in e.items() if not k.startswith("_")}
                if {k: v for k, v in before.items() if k != "yanked"} != \
                        {k: v for k, v in now.items() if k != "yanked"}:
                    self.err(e["_where"], "a published entry changed (only 'yanked' may change)")
        removed = subprocess.run(
            ["git", "-C", self.root, "diff", "--name-only", "--diff-filter=D", self.base, "--", "index"],
            capture_output=True, text=True)
        for rel in removed.stdout.split():
            self.err(rel, "package file was deleted (the index is append-only)")
        return changed

    def check_archives(self, entries):
        for e in entries:
            url, cksum = e.get("url"), e.get("cksum")
            if not isinstance(url, str) or not isinstance(cksum, str):
                continue
            try:
                h = hashlib.sha256()
                total = 0
                with urllib.request.urlopen(url, timeout=60) as resp:
                    while True:
                        chunk = resp.read(1 << 16)
                        if not chunk:
                            break
                        total += len(chunk)
                        if total > MAX_ARCHIVE_BYTES:
                            raise ValueError("archive larger than 64 MiB")
                        h.update(chunk)
                if h.hexdigest() != cksum:
                    self.err(e["_where"], f"archive at {url} has SHA-256 {h.hexdigest()}, "
                                          f"entry says {cksum}")
            except (OSError, ValueError) as ex:
                self.err(e["_where"], f"cannot download archive {url}: {ex}")

    def run(self):
        self.load()
        self.check_cross()
        self.check_search_index()
        changed = self.changed_entries()
        if not self.offline:
            self.check_archives(changed)
        return self.problems


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--root", default=".", help="index repository root")
    ap.add_argument("--base", help="git ref to diff against (enforces append-only history)")
    ap.add_argument("--offline", action="store_true", help="skip archive checksum downloads")
    ap.add_argument("--require-signatures", action="store_true")
    ap.add_argument("--allow-insecure-urls", action="store_true",
                    help="accept http:// and file:// archive URLs (tests, private mirrors)")
    args = ap.parse_args(argv)
    v = Validator(args.root, base=args.base, offline=args.offline,
                  require_signatures=args.require_signatures,
                  allow_insecure_urls=args.allow_insecure_urls)
    problems = v.run()
    for p in problems:
        print(f"error: {p}")
    count = sum(len(e) for e in v.packages.values())
    if problems:
        print(f"{len(problems)} problem(s) in {len(v.packages)} package(s)")
        return 1
    print(f"ok: {len(v.packages)} package(s), {count} version(s)")
    return 0


if __name__ == "__main__":
    sys.exit(main())

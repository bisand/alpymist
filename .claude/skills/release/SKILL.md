---
name: release
description: Cut an Alpymist release — bump the version everywhere, tag it, and let CI build the ISOs and publish to the stable channel. Use when asked to do a release, cut a version, ship a new version, or publish to stable.
---

# Cutting an Alpymist release

One version covers every first-party package; `squint` and `alpymist-keys`
keep their own ([ADR 0008](../../../docs/adr/0008-versions-and-build-numbers.md)).
`pkgrel` is the CI run number — never typed by hand.

## Before anything, decide what is actually in it

```sh
git log v<previous>..HEAD --oneline
```

Read it. If every commit is CI or docs, the release carries no user-visible
change — say so rather than writing notes that imply otherwise. Check whether
the previous release was published to stable: if it was built but never
published, this release carries its contents too, and the notes should lead
with that.

## Steps

1. **Bump.** `cargo xtask version <next>` then `cargo update --workspace`.
   Never edit a `pkgver` by hand.
2. **Verify**, exactly as CI does:
   ```sh
   cargo fmt --all --check
   cargo clippy --workspace --all-targets --features alpymist-ui/render -- -D warnings
   cargo test --workspace --features alpymist-ui/render
   cargo run -q -p xtask -- version --expect v<next>
   ```
3. **Commit and push to main.** Releases are cut from main; `publish` refuses a
   run whose commit is not on it.
4. **Create the release.** Notes are hand-written — read the last two releases
   with `gh release view` first and match their voice.
   ```sh
   gh release create v<next> --target main --prerelease \
     --title "Alpymist <next>" --notes-file <file>
   ```
5. **Watch it through.** Release builds packages and ISOs and attaches them;
   "Publish to stable" then signs the index and pushes the site.
   ```sh
   gh run watch <id> --exit-status
   ```

## Traps

- **`--target` takes a branch.** A commit SHA is rejected with
  `Release.target_commitish is invalid`. Use `--target main`.
- **A version that disagrees with the tag fails the run**, in the `Version` job,
  before anything is built. That is the gate working; fix the version, do not
  retag around it.
- **Pushing to main cancels an in-flight Dev run** — they share the
  `dev-channel` concurrency group. If a Dev run was verifying something, it
  restarts from the beginning.
- **Re-running an old Release run will not publish it.** "Publish to stable"
  fires on a *new* `workflow_run` completion for a release event. A release
  built before that workflow existed has to be published by hand with
  `cargo xtask publish --run <id> --push`.
- **Do not publish a release whose changes have never been built on Alpine.**
  Much of this repo can only be proven in CI. Let a Dev run go green on main
  first; a failed Release leaves a published release with no ISOs attached.

## Verifying the result

```sh
gh release view v<next> --json assets --jq '.assets[].name'
```

Expect four: an `.iso` and an `.iso.sha256` for x86_64 and aarch64. Then confirm
the packages carry the release version with the run number, and that `squint`
and `alpymist-keys` kept their own — `alpymist-0.0.6-r10.apk` alongside
`squint-0.1.8-r0.apk` is what correct looks like.

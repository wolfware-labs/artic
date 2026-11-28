# Contributing to artic

Everyone taking part in the project is expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md). Report security problems privately, as described in [SECURITY.md](SECURITY.md), never in a public issue.

## Setup

1. Install [rustup](https://rustup.rs).
2. Clone the repository. The toolchain version is pinned in `rust-toolchain.toml`, and `rustup` installs it with `rustfmt` and `clippy` the first time you run `cargo` in the repo.
3. Install the extra tools used below:

   ```sh
   cargo install cargo-deny cargo-insta --locked
   ```

The crate layout and the rules for dependencies between crates are in [docs/architecture.md](docs/architecture.md). The design is in [docs/spec/technical_spec.md](docs/spec/technical_spec.md).

## Checks

CI runs these on every pull request. Run them locally before pushing:

| Check | Command | CI job |
| --- | --- | --- |
| Build | `cargo build --workspace --all-targets` | Test (Windows, macOS, Ubuntu) |
| Tests | `cargo test --workspace` | Test (Windows, macOS, Ubuntu) |
| Formatting | `cargo fmt --all --check` | Lint |
| Lints | `cargo clippy --workspace --all-targets -- -D warnings` | Lint |
| Facade features | `cargo test -p artic --all-features` | Test (Ubuntu) |
| Host examples | `cargo run -p artic-examples --example smoke` (with `ARTIC_SMOKE_FRAMES=60`), `--example frames` and `--example hello` (with `ARTIC_HELLO_SECONDS=3`) | Test (Windows, macOS, Ubuntu: under X11 and Wayland) |
| Layering | `cargo test -p docs-check --test layers` and `cargo deny check bans` | Docs, Test, Deny |
| UI-language neutral | `scripts/check-ui-neutral.sh` | Lint |
| Checkout credentials (every `actions/checkout` sets `persist-credentials: false`; needs [yq](https://github.com/mikefarah/yq)) | `scripts/check-checkout-credentials.sh` and its self-test, `scripts/test-check-checkout-credentials.sh` | Lint |
| Locked cargo and token permissions (every cargo call in a `.github` `run:` passes `--locked`, mutants as `--cargo-arg=--locked`, before any word with a character outside letters, digits and `_ . = + / : -` (a safe-character allowlist: variables, quotes, globs and splats may expand), so write it first (`cargo +${{ matrix.rust }} build --locked` is flagged, since an expanding word can move the flag past a `--`), and for run, rustc and rustdoc before the program's first argument, unless it never resolves: fmt, `codspeed run`, version, help, cargo with only a toolchain of letters, digits and `_ . -`, `--version`, `-V`, `--help` or `-h`, and `cargo machete` with only plain paths (letters, digits, `_ . / : -`) and its `--skip-target-dir`, `--fix`, `--no-ignore`, `--version` or `--help` (any other option may be `--with-metadata`, which has no `--locked`, so it's flagged); any call with an option, toolchain or subcommand word holding characters outside letters, digits and `_ . = + / : -` is flagged, exempt subcommands too; every workflow has a top-level `permissions:` block; a cargo call inside quotes or a heredoc, and scripts a step calls, aren't read (except a quoted cargo word after `;`, `|`, `&` or `(`, which is flagged though the shell never runs it), and some shell forms are misread (the script's header lists them); needs [yq](https://github.com/mikefarah/yq)) | `scripts/check-cargo-locked.sh` and its self-test, `scripts/test-check-cargo-locked.sh` | Lint |
| Docs | `cargo doc --workspace --no-deps --exclude docs-check` with `RUSTDOCFLAGS=-D warnings` | Docs |
| Code in documents | `cargo test -p docs-check` | Docs |
| Links in documents | `lychee --offline --include-fragments --root-dir "$PWD" '*.md' 'docs/**/*.md'` | Docs |
| Dependency policy | `cargo deny check` | Deny |
| Mutation testing | `git diff origin/main... > pr.diff`, then `cargo mutants --in-diff pr.diff --in-place`, with `ARTIC_REQUIRE_GPU=1` and `ARTIC_REQUIRE_DISPLAY=1` on a machine with a GPU or lavapipe and an X server (Xvfb or XWayland), since a skipped golden or window test catches no mutant | petri/mutants |

On Windows PowerShell, set the docs flag with `$env:RUSTDOCFLAGS="-D warnings"` before running `cargo doc`.

The required status checks for `main` are `Test (windows-latest)`, `Test (macos-latest)`, `Test (ubuntu-latest)`, `Lint`, `Wasm`, `Docs`, `Deny`, `CodSpeed`, `Coverage gate`, `petri/mutants` and `Socket Security: Pull Request Alerts`. `Wasm` checks that the crates app code uses still build for `wasm32-unknown-unknown`, for a later web host ([web portability](docs/design/web-portability.md)). Socket checks the dependencies a pull request adds or changes for known vulnerabilities and supply-chain risks. `petri/mutants` comes from petri, which mutates the lines a pull request changes on its own runners and fails when a mutant survives, that is when no test fails with it; a mutant that makes the tests hang is listed and passes. On a pull request from a fork, petri runs nothing until an organization admin or a code owner comments `/petri run <full head sha>` naming the head, and then runs it without any token of the repository. A weekly workflow, `mutants-full.yml`, mutates the whole workspace on GitHub's runners. The Ubuntu job's `Test` step runs on lavapipe, a Vulkan driver on the CPU, and Xvfb, a virtual X server, with `ARTIC_REQUIRE_GPU=1` and `ARTIC_REQUIRE_DISPLAY=1`, so the goldens and the window test run there instead of skipping. The Windows job runs the goldens on lavapipe too, from Mesa's Windows build, since vello refuses the runners' WARP adapter; the macOS job runs them on the runners' Metal adapter. On Ubuntu the host examples run under Xvfb and again under weston, a Wayland compositor with no screen, where `smoke`'s log must name the display server it got and `frames`' the lavapipe adapter.

## Golden images

Rendering tests compare an image against a PNG checked in beside the test. On a mismatch, `<name>.actual.png` and `<name>.diff.png` land beside the golden, and CI keeps them as an artifact of the failed job (`goldens-Linux`, `goldens-Windows` or `goldens-macOS`). When a change to rendering is intended, write the new goldens with `ARTIC_UPDATE_GOLDENS=1 cargo test --workspace` on a machine with a GPU or lavapipe, review the PNGs in the pull request, and commit them. One set of goldens serves all three platforms, and the tests draw text with the fonts in `artic-test-fonts`, not the system's, so a golden that passes on one platform and fails on another shows a renderer difference: look at that job's diff before writing anything.

## Snapshot tests

Parser and compiler tests use [insta](https://insta.rs) snapshots. When a change alters a snapshot, the test fails and writes a `.snap.new` file next to it. Review and accept the changes with:

```sh
cargo insta review
```

Commit the updated `.snap` files with the change that caused them. Never commit `.snap.new` files.

## Errors

An error's message says what failed at its own layer and never formats its cause. The cause is `#[source]` (or `#[from]`, or the whole error is `transparent`), so a chain reporter such as `anyhow` prints each cause once. Code that shows an error to a person, in a panic, a log line or a `main`, shows `artic_core::error::Chain(&error)`, which prints the message and then each `source()`.

## Dependencies

`deny.toml` defines the dependency policy. Dependencies must use one of the allowed licenses, come from crates.io, and have no open security advisories, including unsound ones in dependencies of dependencies. A weekly workflow, `advisories.yml`, checks the lockfile as well, since advisories appear without any change here. A new dependency that fails `cargo deny check` needs a discussion in its pull request before the policy changes.

A new dependency between workspace crates must follow the layers in [docs/architecture.md](docs/architecture.md). Update that document in the same pull request.

## Issues and labels

Every issue has three kinds of labels:

| Prefix | Meaning | Example |
| --- | --- | --- |
| `type:` | Kind of work: `feature`, `bug`, `test`, `infra`, `docs`, `spike`, `design` | `type:feature` |
| `crate:` | Crates the work touches; `crate:workspace` for repo-wide changes | `crate:artic-core` |
| `platform:` | Only when the work is specific to a platform | `platform:android` |

Reports from the issue forms also get `needs-triage` until a maintainer reviews them. Questions and ideas belong in [Discussions](https://github.com/wolfware-labs/artic/discussions).

Issues are grouped into milestones. The "Depends on" line in an issue lists what has to land first.

## Pull requests

- Write the pull request title as a conventional commit (see below). Pull requests are squash merged, and the title becomes the commit on `main`.
- Keep one issue per pull request, and put `Closes #<number>` in the description.
- Keep unrelated changes out, including drive-by formatting.
- Add or update tests for the behavior you change.
- Update the docs and the spec when the change affects them.
- Make sure every check above passes.

## Commit messages

Pull request titles and commits follow [Conventional Commits](https://www.conventionalcommits.org):

```text
<type>(<scope>): <summary>
```

- `type` is one of `feat`, `fix`, `refactor`, `perf`, `test`, `docs`, `build`, `ci`, `style` or `chore`.
- `scope` is the crate without the `artic-` prefix (`core`, `xaml`, `controls`), and can be left out for repo-wide changes.
- The summary is in the imperative mood ("add", "fix", "remove"), lowercase, at most 72 characters, with no trailing period.
- Add a body after a blank line when the change needs explaining: what changed and why, wrapped at 72 characters.
- Reference issues in the pull request description, not in the subject line.

```text
feat(layout): add Grid star sizing

Distribute the remaining space across star rows and columns in
proportion to their weights, after Auto and fixed sizes are measured.
```

Commits inside a pull request can be less formal, since only the title reaches `main`, but the same format keeps the history easy to read.

## Contributor License Agreement

Every contributor signs the [Contributor License Agreement](CLA.md) once. You keep the copyright to your contribution.

When you open your first pull request, a bot comments with a link to the agreement. Reply with the sentence it asks for, and the check passes. Pull requests cannot be merged until every author has signed.

## License

By contributing, you agree that your contributions are dual licensed under the MIT and Apache 2.0 licenses, as described in the [README](README.md#license).

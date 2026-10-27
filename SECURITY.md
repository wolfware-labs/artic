# Security policy

## Supported versions

artic has no stable release yet. Security fixes land on `main` and in the latest published version once releases start.

## Reporting a vulnerability

Report vulnerabilities privately through GitHub: open the repository's **Security** tab and choose **Report a vulnerability**. Do not open a public issue or discussion for a security problem.

A useful report includes:

- the affected crate and version (or commit),
- the platform, if the issue is platform-specific,
- steps or a minimal example that reproduce the problem,
- the impact you expect, for example code execution, data exposure or a crash from untrusted XAML.

You can expect a first reply within 7 days. Once a fix is ready, the advisory is published together with the release that contains it, and reporters are credited unless they ask not to be.

## Scope

Loading untrusted XAML with the `xaml-runtime` feature is in scope: the loader is meant to reject hostile input without panicking, hanging or exhausting memory. Bugs in third-party dependencies should be reported upstream, but let us know if artic is affected.

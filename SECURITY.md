# Security policy

## Reporting a vulnerability

Do not open a public issue for a security problem.

Write to support@klaracase.de and say that the report is about the Rust SDK. Describe the problem,
the version of the crate and the steps to reproduce it. We will answer and tell you what happens next.

## Scope

This policy covers the code in this repository. For a problem in the Klaracase service itself, use the
same address.

## Your API token

An API token is a secret. Do not commit it, log it or send it to a third party. If a token leaks,
delete it at https://klaracase.de/settings/api-tokens and create a new one.

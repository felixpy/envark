# Security policy

## Supported versions

Security fixes target the latest stable release. Older releases and development builds do not receive separate security updates.

## Report a vulnerability

Please use GitHub's [private vulnerability reporting](https://github.com/felixpy/envark/security/advisories/new). Do not disclose exploit details in a public issue or pull request before maintainers have investigated the report and coordinated a fix.

Include what you can safely share:

- Envark version, operating system, and architecture.
- A minimal reproduction and the affected runtime or tool manager.
- Expected behavior, observed behavior, and potential impact.
- Relevant logs or screenshots, with credentials and personal information removed.

Security-sensitive areas include command execution, installation ownership, cleanup path validation, and access to configuration containing secrets. If a report involves cleanup, use disposable sample directories for reproduction.

Maintainers will investigate privately and coordinate disclosure with the reporter. There is no guaranteed response time or paid bounty program.

For ordinary bugs and support questions, follow [SUPPORT.md](SUPPORT.md).

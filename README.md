![example workflow](https://github.com/RyanSquared/webhook-runner/actions/workflows/ci.yaml/badge.svg)

# webhook-runner
rust program to run thing once webhook is hit

## Usage:

```
% cargo run -- --help
webhook-runner 0.1.0
Run commands based on optionally signed commits from a Git repository

USAGE:
    webhook-runner [OPTIONS]

OPTIONS:
    -b, --bind-address <BIND_ADDRESS>
            Address to bind to; only accepts one argument, for multiple bind addresses use a reverse
            proxy [env: BIND_ADDRESS=] [default: 0.0.0.0:80]

        --clone-timeout <CLONE_TIMEOUT>
            UNSTABLE: Timeout for `git clone` in seconds [env: CLONE_TIMEOUT=] [default: 4294967295]

        --command-timeout <COMMAND_TIMEOUT>
            UNSTABLE: Timeout for commands run by webhooks in seconds [env: COMMAND_TIMEOUT=]
            [default: 4294967295]

        --commit-command <COMMIT_COMMAND>
            UNSTABLE: Shell command to run after commits are (optionally) verified [env:
            COMMIT_COMMAND=]

        --commit-keyring <COMMIT_KEYRING>
            UNSTABLE: PGP keyring file for verifying commits [env: COMMIT_KEYRING=]

        --git-repository <GIT_REPOSITORY>
            Remote address of the Git repository; supports any format Git supports, such as
            `git@github.com:RyanSquared/webhook-runner` [env: GIT_REPOSITORY=]

    -h, --help
            Print help information

        --ssh-key <SSH_KEY>
            Full path to file of an SSH key that should be used when a Git repository with an SSH
            URL is configured [env: SSH_KEY=]

        --tag-command <TAG_COMMAND>
            UNSTABLE: Shell command to run after tags are (optionally) verified [env: TAG_COMMAND=]

        --tag-keyring <TAG_KEYRING>
            UNSTABLE: PGP keyring file for verifying tags [env: TAG_KEYRING=]

    -V, --version
            Print version information

        --webhook-secret-key <WEBHOOK_SECRET_KEY>
            UNSTABLE: 256-bit secret key for verifying GitHub webhooks [env: WEBHOOK_SECRET_KEY=]
```

See [TODO.md] for more information about what is planned.

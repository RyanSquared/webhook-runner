- [ ] Run commit commands only on specified branch(es?)
- [ ] Export metadata about the repository such as tag commands through
  environment variables
- [ ] Set the working directory for subcommands to the repository directory
- [ ] Actually run commands(‽)
  - [ ] Move command invocation to background thread pool
  - [ ] Return unavailable if thread pool does not have any available threads
  - [X] Keep verification in same thread as worker so GitHub gets a response
- [ ] Configure option to report command failures to some webhook
- [ ] Extract components into their own crates in workspace
  - Result: Separates the Git and runner components from the webhook components
  - Rationale: If we need to change to a new webhook or runner system, only one
    component needs to be replaced.
